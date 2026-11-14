import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { EngineClient, EngineError } from './engine';
import type { EditorSurfaceHandle } from './EditorSurface';
import { PagedEditor, type PageInfo } from './PagedEditor';
import { PagePreview, type LayoutOrigin, type PreviewZoom } from './PagePreview';
import { WriterIcon } from './WriterIcon';
import { FindBar } from './FindBar';
import { StorageError, WriterStorage, type StoredDocument } from './storage';
import { WriterSaveController, type SaveAssociation, type SaveState } from './autosave';
import type { Alignment, CompatibilityReport, LayoutAction, WriterCommand, WriterView } from './types';
import './styles.css';
import fontLicensesUrl from '../../../crates/office-layout/fonts/NOTICE.txt?url';

function message(error: unknown): string {
  return error instanceof Error ? error.message : 'The operation could not be completed.';
}

function Dialog({ title, children, onClose, busy = false }: {
  title: string; children: ReactNode; onClose: () => void; busy?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const label = useId();
  useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);
  return <dialog ref={ref} aria-labelledby={label} onCancel={(event) => {
    event.preventDefault();
    if (!busy) onClose();
  }}>
    <div className="dialog-heading"><h2 id={label}>{title}</h2>
      <button aria-label="Close dialog" className="icon-button" disabled={busy} onClick={onClose}>×</button>
    </div>
    {children}
  </dialog>;
}

function AlignmentIcon({ alignment }: { alignment: Alignment }) {
  const lengths = alignment === 'justify' ? [18, 18, 18, 18] : [18, 12, 18, 9];
  return <svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24">
    {lengths.map((length, index) => <path key={index}
      d={`M${alignment === 'right' ? 21 - length : alignment === 'center' ? (24 - length) / 2 : 3} ${5 + index * 5}h${length}`}
      stroke="currentColor" strokeWidth="1.6" />)}
  </svg>;
}

type LibraryEntry = Pick<StoredDocument, 'id' | 'title' | 'etag' | 'updatedAt'>;
type PreservedImport = { file: File; message: string; report: CompatibilityReport | undefined };

const DOCX_MEDIA_TYPE = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';

function download(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function safeFilename(title: string, extension: string): string {
  const name = [...(title.trim() || 'Untitled document')]
    .map((character) => character.charCodeAt(0) < 32 || '\\/:*?"<>|'.includes(character) ? '_' : character).join('').slice(0, 100);
  return `${name}.${extension}`;
}

export default function App() {
  const engine = useRef<EngineClient | null>(null);
  const storage = useRef<WriterStorage | null>(null);
  const surface = useRef<EditorSurfaceHandle>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const documentMenu = useRef<HTMLDetailsElement>(null);
  const findPanel = useRef<HTMLDivElement>(null);
  const pendingFindFocus = useRef(false);
  const replacementFocus = useRef<{ previous: Element | null } | null>(null);
  const viewRef = useRef<WriterView | null>(null);
  const dirtyRef = useRef(false);
  const documentOpenRef = useRef(true);
  const saveController = useRef<WriterSaveController | null>(null);
  const pending = useRef<(() => void | Promise<void>) | null>(null);
  const [view, setView] = useState<WriterView | null>(null);
  const [startupError, setStartupError] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [dirty, setDirtyState] = useState(false);
  const [documentOpen, setDocumentOpen] = useState(true);
  const [busy, setBusy] = useState(false);
  const [opening, setOpening] = useState(false);
  const [savedLocally, setSavedLocally] = useState(false);
  const [saveState, setSaveState] = useState<SaveState>({
    phase: 'idle', dirty: false, savedLocally: false, composing: false, error: null,
  });
  const [composing, setComposing] = useState(false);
  const [library, setLibrary] = useState<readonly LibraryEntry[] | null>(null);
  const [libraryProblems, setLibraryProblems] = useState<readonly { message: string; id?: string }[]>([]);
  const [unsavedDialog, setUnsavedDialog] = useState(false);
  const [saveConflict, setSaveConflict] = useState(false);
  const [exportDisclosure, setExportDisclosure] = useState(false);
  const [preservedImport, setPreservedImport] = useState<PreservedImport | null>(null);
  const [pagePreview, setPagePreview] = useState(false);
  const [previewZoom, setPreviewZoom] = useState<PreviewZoom>('fit');
  const [moreFormatting, setMoreFormatting] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<'write' | 'insert' | 'layout' | 'review'>('write');
  const [pageInfo, setPageInfo] = useState<PageInfo>({ pageCount: null, currentPage: null, status: 'loading', zoomPercent: null });
  const editingPageInfo = useRef(pageInfo);
  const previousPreview = useRef(false);

  useLayoutEffect(() => {
    const enteringPreview = !previousPreview.current && pagePreview;
    const returningToEditing = previousPreview.current && !pagePreview;
    previousPreview.current = pagePreview;
    if (!documentOpen || !documentOpenRef.current) return;
    if (enteringPreview) {
      setPageInfo({ pageCount: null, currentPage: null, status: 'loading', zoomPercent: null });
      // This is a fixed interface root, independent of native document IDs.
      document.getElementById('writer-page-preview')?.focus({ preventScroll: true });
    } else if (returningToEditing) {
      setPageInfo(editingPageInfo.current);
      // Restore the Rust selection in the same commit that reveals the editor.
      // Deferring focus would let the first keyboard event target a removed
      // preview control, or steal focus from a control reached afterward.
      surface.current?.focus();
    }
  }, [documentOpen, pagePreview]);

  useLayoutEffect(() => {
    if (!pendingFindFocus.current || !findOpen || pagePreview) return;
    pendingFindFocus.current = false;
    findPanel.current?.querySelector('input')?.focus({ preventScroll: true });
  }, [findOpen, pagePreview]);

  useLayoutEffect(() => {
    const requested = replacementFocus.current;
    if (!requested || !documentOpen || !documentOpenRef.current) return;
    replacementFocus.current = null;
    if (document.activeElement !== requested.previous && document.activeElement !== document.body) return;
    if (pagePreview) document.getElementById('writer-page-preview')?.focus({ preventScroll: true });
    else surface.current?.focus();
  }, [documentOpen, pagePreview, view]);

  const loadPageLayout = useCallback((origin: LayoutOrigin) => {
    const client = engine.current;
    if (!client || !documentOpenRef.current || client.view.sessionId !== origin.sessionId
      || client.view.revision !== origin.revision || client.view.documentId !== origin.documentId) {
      throw new Error('The document changed while preparing page preview. Return to editing and try again.');
    }
    return client.layout();
  }, []);

  const loadSelectionGeometry = useCallback((origin: LayoutOrigin) => {
    const client = engine.current;
    if (!client || !documentOpenRef.current || client.view.sessionId !== origin.sessionId
      || client.view.revision !== origin.revision || client.view.documentId !== origin.documentId) {
      throw new Error('The document changed while preparing its selection. Try the action again.');
    }
    return client.selectionGeometry();
  }, []);

  const findText = useCallback((query: string) => {
    if (!engine.current || !documentOpenRef.current) throw new Error('Open a document before searching.');
    return engine.current.find(query);
  }, []);

  const acceptEditingPageInfo = useCallback((info: PageInfo) => {
    editingPageInfo.current = info;
    if (!previousPreview.current && documentOpenRef.current) setPageInfo(info);
  }, []);

  const acceptPreviewPageInfo = useCallback((info: PageInfo) => {
    if (previousPreview.current && documentOpenRef.current) setPageInfo(info);
  }, []);

  function togglePagePreview() {
    if (!viewRef.current || !documentOpenRef.current || busy || composing) return;
    setPagePreview((active) => !active);
  }

  function setDirty(value: boolean) {
    dirtyRef.current = value;
    setDirtyState(value);
  }

  function updateView(next: WriterView, trackChange = true) {
    const previous = viewRef.current;
    const changed = trackChange && (previous?.revision !== next.revision
      || previous.sessionId !== next.sessionId || previous.documentId !== next.documentId);
    viewRef.current = next;
    setView(next);
    if (changed) {
      setDirty(true);
      saveController.current?.changed();
    }
    return next;
  }

  useEffect(() => {
    let canceled = false;
    let client: EngineClient | null = null;
    const localStore = new WriterStorage();
    storage.current = localStore;
    const saver = new WriterSaveController({
      storage: localStore,
      current: () => {
        const active = engine.current;
        const current = viewRef.current;
        if (!active || !current || !documentOpenRef.current) return null;
        return { sessionId: current.sessionId, documentId: current.documentId,
          revision: current.revision, title: current.title, serialize: () => active.serialize() };
      },
      onState: (next) => {
        setSaveState(next);
        dirtyRef.current = next.dirty;
        setDirtyState(next.dirty);
        setSavedLocally(next.savedLocally);
        setSaveConflict(next.error instanceof StorageError && next.error.code === 'conflict');
      },
      onCommit: (commit) => {
        if (commit.current) setNotice('Saved on this device');
      },
    });
    saveController.current = saver;
    void EngineClient.create().then((created) => {
      client = created;
      if (canceled) { created.dispose(); return; }
      engine.current = created;
      viewRef.current = created.view;
      setView(created.view);
      saver.reset(null);
    }).catch((failure: unknown) => {
      if (!canceled) setStartupError(message(failure));
    });
    return () => {
      canceled = true;
      saver.dispose();
      saveController.current = null;
      engine.current?.dispose();
      client?.dispose();
      engine.current = null;
      localStore.close();
    };
  }, []);

  useEffect(() => {
    function beforeUnload(event: BeforeUnloadEvent) {
      if (dirtyRef.current) { event.preventDefault(); event.returnValue = ''; }
    }
    window.addEventListener('beforeunload', beforeUnload);
    function visibilityChanged() {
      if (document.visibilityState === 'hidden') void saveController.current?.flushPending();
    }
    document.addEventListener('visibilitychange', visibilityChanged);
    return () => {
      window.removeEventListener('beforeunload', beforeUnload);
      document.removeEventListener('visibilitychange', visibilityChanged);
    };
  }, []);

  useEffect(() => {
    function closeMenu(event: PointerEvent) {
      if (event.target instanceof Node && !documentMenu.current?.contains(event.target)) {
        documentMenu.current?.removeAttribute('open');
      }
    }
    document.addEventListener('pointerdown', closeMenu);
    return () => document.removeEventListener('pointerdown', closeMenu);
  }, []);

  function compositionChanged(active: boolean) {
    setComposing(active);
    saveController.current?.setComposing(active);
  }

  function command(input: WriterCommand): WriterView | undefined {
    try {
      if (!engine.current) return;
      const next = updateView(engine.current.command(input));
      setError('');
      return next;
    } catch (failure) {
      setError(message(failure));
      return;
    }
  }

  function interact(action: LayoutAction, expectedOrigin?: LayoutOrigin): WriterView | undefined {
    try {
      if (!engine.current) return;
      const next = updateView(engine.current.interact(action, {}, expectedOrigin));
      setError('');
      return next;
    } catch (failure) {
      setError(message(failure));
      return;
    }
  }

  function documentAction(action: () => void) {
    documentMenu.current?.removeAttribute('open');
    action();
  }

  function openFind() {
    if (findOpen && !pagePreview) {
      findPanel.current?.querySelector('input')?.focus({ preventScroll: true });
      return;
    }
    pendingFindFocus.current = true;
    setPagePreview(false);
    setFindOpen(true);
  }

  function changeZoom(direction: -1 | 1) {
    const levels: readonly PreviewZoom[] = ['50', '75', '100', '125', '150', '200'];
    const index = previewZoom === 'fit' ? 2 : levels.indexOf(previewZoom);
    setPreviewZoom(levels[Math.max(0, Math.min(levels.length - 1, index + direction))] ?? '100');
  }

  function replaceSession(next: WriterView, source: SaveAssociation | null, info: string) {
    replacementFocus.current = { previous: document.activeElement };
    setSavedLocally(source !== null);
    updateView(next, false);
    setDirty(false);
    documentOpenRef.current = true;
    saveController.current?.reset(source);
    setDocumentOpen(true);
    setSaveConflict(false);
    setError('');
    setNotice(info);
    setLibrary(null);
    setFindOpen(false);
  }

  function guardReplacement(action: () => void | Promise<void>) {
    if (composing || busy) return;
    if (dirtyRef.current) {
      pending.current = action;
      setUnsavedDialog(true);
    } else void action();
  }

  function newDocument() {
    guardReplacement(() => {
      if (engine.current) replaceSession(engine.current.command({ type: 'new_document' }), null, 'New document');
    });
  }

  async function saveDocument(): Promise<boolean> {
    if (!saveController.current || !engine.current || !viewRef.current || !documentOpen || composing || busy) return false;
    setBusy(true);
    setError('');
    try {
      return await saveController.current.flush();
    } catch (failure) {
      setError(`${message(failure)} Your document is still open and unchanged. You can also export a Writer file.`);
      return false;
    } finally { setBusy(false); }
  }

  async function saveCopy(): Promise<boolean> {
    if (!engine.current || busy || composing) return false;
    try {
      const next = engine.current.command({ type: 'fork_document' });
      setSavedLocally(false);
      updateView(next);
      setSaveConflict(false);
      return await saveDocument();
    } catch (failure) { setError(message(failure)); return false; }
  }

  async function showLibrary() {
    if (!storage.current || busy || composing) return;
    setBusy(true);
    try {
      const listing = await storage.current.list();
      setLibrary(listing.documents);
      setLibraryProblems(listing.problems);
      setError('');
    } catch (failure) { setError(message(failure)); }
    finally { setBusy(false); }
  }

  function openSaved(id: string) {
    guardReplacement(async () => {
      if (!storage.current || !engine.current) return;
      const origin = viewRef.current;
      setBusy(true);
      setOpening(true);
      try {
        const record = await storage.current.load(id);
        if (viewRef.current?.revision !== origin?.revision || viewRef.current?.sessionId !== origin?.sessionId) {
          throw new Error('Your document changed while opening. Try opening again after saving your changes.');
        }
        const next = engine.current.open(record.bytes, record.id);
        replaceSession(next, { sessionId: next.sessionId, id: record.id, etag: record.etag }, 'Opened from this device');
      } catch (failure) { setError(message(failure)); }
      finally { setBusy(false); setOpening(false); }
    });
  }

  function importFile(file: File) {
    guardReplacement(async () => {
      const active = engine.current;
      const local = storage.current;
      if (!active) return;
      const origin = viewRef.current;
      const wasOpen = documentOpenRef.current;
      const isDocx = /\.docx$/i.test(file.name) || file.type === DOCX_MEDIA_TYPE;
      let candidate: EngineClient | null = null;
      const checkOrigin = () => {
        if (engine.current !== active || documentOpenRef.current !== wasOpen
          || viewRef.current?.revision !== origin?.revision || viewRef.current?.sessionId !== origin?.sessionId) {
          throw new Error('Your document changed while importing. Try again after saving your changes.');
        }
      };
      setBusy(true);
      setOpening(true);
      setError('');
      try {
        if (file.size > 5 * 1024 * 1024) throw new Error('This prototype opens files up to 5 MiB, including retained originals.');
        const bytes = new Uint8Array(await file.arrayBuffer());
        checkOrigin();
        if (isDocx) {
          if (!local) throw new Error('Local storage is required to preserve the original before editing a DOCX.');
          candidate = await EngineClient.create();
          checkOrigin();
          const next = candidate.importDocx(bytes, file.name);
          const native = candidate.serialize();
          // A supported import is not activated until the native document and
          // exact original have completed one existing IndexedDB transaction.
          const saved = await local.save({ id: next.documentId, title: next.title, bytes: native }, null);
          checkOrigin();
          engine.current = candidate;
          candidate = null;
          replaceSession(next, { sessionId: next.sessionId, id: saved.id, etag: saved.etag },
            'DOCX imported. The document and its exact original are saved on this device.');
          active.dispose();
        } else {
          const next = active.open(bytes);
          replaceSession(next, null, 'Imported Writer file. Save a local copy to keep it on this device.');
        }
      } catch (failure) {
        if (isDocx && failure instanceof EngineError && failure.code === 'unsupported_docx') {
          setPreservedImport({ file, message: failure.message, report: failure.compatibility });
          setLibrary(null);
        } else setError(`Could not open this file. ${message(failure)}`);
      } finally {
        candidate?.dispose();
        setBusy(false);
        setOpening(false);
      }
    });
  }

  function exportWriterFile() {
    if (!engine.current || !viewRef.current || !documentOpen || composing) return;
    try {
      const bytes = engine.current.serialize();
      download(new Blob([bytes.slice().buffer], { type: 'application/json;charset=utf-8' }), safeFilename(viewRef.current.title, 'writer'));
      setExportDisclosure(false);
      setNotice('Writer file exported. Export does not replace your local save.');
    } catch (failure) { setError(message(failure)); }
  }

  function exportFile() {
    if (viewRef.current?.originalSource) {
      setError('');
      setExportDisclosure(true);
    }
    else exportWriterFile();
  }

  function exportWordFile() {
    if (!engine.current || !viewRef.current || !documentOpen || composing) return;
    try {
      const bytes = engine.current.exportDocx();
      download(new Blob([bytes.slice().buffer], { type: DOCX_MEDIA_TYPE }), safeFilename(viewRef.current.title, 'docx'));
      setNotice('Word file exported from current supported content. Export does not replace your local save.');
    } catch (failure) { setError(message(failure)); }
  }

  function downloadOriginal() {
    const source = viewRef.current?.originalSource;
    if (!engine.current || !source || !documentOpen || composing) return;
    try {
      const bytes = engine.current.exportOriginal();
      download(new Blob([bytes.slice().buffer], { type: DOCX_MEDIA_TYPE }), source.filename);
      setNotice('Exact original downloaded. Later edits are available in Writer and Word exports.');
    } catch (failure) { setError(message(failure)); }
  }

  function closeDocument() {
    guardReplacement(() => {
      setPagePreview(false);
      setFindOpen(false);
      documentOpenRef.current = false;
      saveController.current?.reset(null);
      setDocumentOpen(false);
      setDirty(false);
      setNotice('Document closed');
    });
  }

  function finishPending() {
    const action = pending.current;
    pending.current = null;
    setUnsavedDialog(false);
    void action?.();
  }

  useEffect(() => {
    function shortcut(event: KeyboardEvent) {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || composing) return;
      if (event.key.toLowerCase() === 's') { event.preventDefault(); void saveDocument(); }
      if (event.key.toLowerCase() === 'o') { event.preventDefault(); void showLibrary(); }
      if (event.key.toLowerCase() === 'f' && documentOpen && view) {
        event.preventDefault();
        openFind();
      }
    }
    window.addEventListener('keydown', shortcut);
    return () => window.removeEventListener('keydown', shortcut);
  });

  function format(input: WriterCommand, refocus = true) {
    const next = command(input);
    // Focus synchronously; a deferred callback could steal focus from the next
    // keyboard control. The surface restores the new Rust selection on update.
    if (refocus && next) surface.current?.focus();
  }

  const unavailable = !view || !documentOpen || busy || composing;
  const formatState = view?.format;
  const visibleError = error || (saveState.error
    ? `${message(saveState.error)} Autosave is paused. Your document is still open; retry Save or export a backup.` : '');
  const saveLabel = !documentOpen ? 'No document open'
    : saveState.phase === 'error' ? 'Unsaved · autosave paused'
      : saveState.composing ? 'Composing · not saved'
        : saveState.phase === 'saving' ? 'Saving on this device…'
          : saveState.phase === 'pending' ? 'Unsaved changes · saving soon'
            : dirty ? 'Unsaved changes' : savedLocally ? 'Saved on this device' : 'Not saved yet';
  return <div className="writer-app">
    <header className="top-bar">
      <div className="identity" role="img" aria-label="Writer"><WriterIcon name="lantern" className="writer-lantern" /><span>Writer</span></div>
      <div className="title-area">
        <input aria-label="Document title" className="document-title" placeholder="Untitled document"
          value={documentOpen ? view?.title ?? '' : ''} disabled={unavailable} maxLength={250}
          onChange={(event) => command({ type: 'set_title', title: event.target.value })} />
        <span className="mobile-save-state" aria-live="polite">{saveLabel}</span>
      </div>
      <nav className="file-actions" aria-label="Document actions">
        <button className="desktop-action" disabled={!view || busy || composing} onClick={newDocument}><WriterIcon name="new" /><span>New</span></button>
        <button className="desktop-action" disabled={!view || busy || composing} onClick={() => void showLibrary()}><WriterIcon name="open" /><span>Open</span></button>
        <button className="find-action" aria-label="Find" title="Find in document (Ctrl/⌘ F)" disabled={unavailable}
          onClick={openFind}><WriterIcon name="search" /></button>
        <button className="device-save" aria-label="Save" title="Save on this device (Ctrl/⌘ S)" disabled={unavailable}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => void saveDocument()}><WriterIcon name="save" /><span className="desktop-label">Save</span></button>
        <details className="document-menu" ref={documentMenu} onBlur={(event) => {
          if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) {
            documentMenu.current?.removeAttribute('open');
          }
        }} onKeyDown={(event) => {
          if (event.key === 'Escape') { documentMenu.current?.removeAttribute('open'); documentMenu.current?.querySelector('summary')?.focus(); }
        }}>
          <summary aria-label="Document options" title="Document options"><WriterIcon name="more" /></summary>
          <div className="document-menu-panel" aria-label="Document options">
            <span className="menu-caption">This device</span>
            <button disabled={!view || busy || composing} onClick={() => documentAction(newDocument)}>New document</button>
            <button disabled={!view || busy || composing} onClick={() => documentAction(() => void showLibrary())}>Open document</button>
            <button disabled={unavailable} onClick={() => documentAction(exportFile)}>Export</button>
            <button disabled={unavailable} onClick={() => documentAction(exportWordFile)}>Export Word</button>
            {view?.originalSource && <button disabled={unavailable} onClick={() => documentAction(downloadOriginal)}>Download original</button>}
            <button disabled={unavailable} onClick={() => documentAction(togglePagePreview)} aria-pressed={pagePreview}>Page preview</button>
            <button disabled={unavailable} onClick={() => documentAction(closeDocument)}>Close</button>
            <a href={fontLicensesUrl} target="_blank" rel="noreferrer">Font licenses</a>
          </div>
        </details>
      </nav>
    </header>
    <nav className="editor-tabs" aria-label="Editor sections">
      {(['write', 'insert', 'layout', 'review'] as const).map((tab) => <button key={tab}
        aria-pressed={activeTab === tab} aria-controls="writer-formatting" onClick={() => setActiveTab(tab)}>
        {tab.charAt(0).toUpperCase() + tab.slice(1)}</button>)}
      <span className={`save-state${dirty ? ' is-unsaved' : ''}`} aria-live="polite">
        {saveLabel}
      </span>
    </nav>
    <div id="writer-formatting" className={`formatting-bar${moreFormatting ? ' show-more-formatting' : ''}`} aria-label={`${activeTab.charAt(0).toUpperCase() + activeTab.slice(1)} controls`}>
      <div className="history-controls" role="group" aria-label="History">
        <button aria-label="Undo" title="Undo (Ctrl/⌘ Z)" disabled={unavailable || !view?.canUndo}
          onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'undo' })}><WriterIcon name="undo" /></button>
        <button aria-label="Redo" title="Redo (Ctrl/⌘ Shift Z)" disabled={unavailable || !view?.canRedo}
          onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'redo' })}><WriterIcon name="redo" /></button>
      </div>
      {activeTab === 'write' && <>
        <fieldset disabled={unavailable} className="format-group paragraph-style-control">
          <legend className="visually-hidden">Paragraph style</legend>
          <label className="visually-hidden" htmlFor="paragraph-style">Paragraph style</label>
          <select id="paragraph-style" aria-label="Paragraph style" title="Apply a named style, or Paragraph for direct formatting"
            value={formatState?.styleMixed ? 'mixed' : JSON.stringify(formatState?.styleId ?? null)}
            onChange={(event) => format({ type: 'assign_style', styleId: JSON.parse(event.target.value) as string | null }, false)}>
            {formatState?.styleMixed && <option value="mixed" disabled>Mixed styles</option>}
            <option value="null">Paragraph</option>
            {view?.styles.map((style) => <option key={style.id} value={JSON.stringify(style.id)}>{style.name}</option>)}
          </select>
        </fieldset>
        <fieldset disabled={unavailable} className="format-group font-selectors advanced-formatting">
          <legend className="visually-hidden">Font</legend>
          <label className="visually-hidden" htmlFor="font-family">Font family</label>
          <select id="font-family" aria-label="Font family" value={formatState?.fontFamily ?? ''}
            onChange={(event) => format({ type: 'format_character', fontFamily: event.target.value }, false)}>
            <option value="" disabled>Mixed</option>
            {[...new Set(['Arimo', 'Tinos', 'Noto Sans Arabic', 'Noto Sans Devanagari', 'Noto Sans Hebrew', 'Noto Emoji', ...(formatState?.fontFamily ? [formatState.fontFamily] : [])])].map((font) => <option key={font}>{font}</option>)}
          </select>
          <label className="visually-hidden" htmlFor="font-size">Font size</label>
          <select id="font-size" aria-label="Font size" value={formatState?.fontSizePt ?? ''}
            onChange={(event) => format({ type: 'format_character', fontSizePt: Number(event.target.value) }, false)}>
            <option value="" disabled>—</option>
            {[...new Set([8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72, ...(formatState?.fontSizePt ? [formatState.fontSizePt] : [])])].sort((a, b) => a - b).map((size) => <option key={size} value={size}>{size}</option>)}
          </select>
        </fieldset>
        <fieldset disabled={unavailable} className="format-group character-controls">
          <legend className="visually-hidden">Character formatting</legend>
          {(['bold', 'italic', 'underline'] as const).map((property) => <button key={property}
            className={`format-button format-${property}`} aria-label={property.charAt(0).toUpperCase() + property.slice(1)}
            aria-pressed={formatState?.[property] === null ? 'mixed' : !!formatState?.[property]}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => format({ type: 'format_character', [property]: !formatState?.[property] })}>{property === 'bold' ? 'B' : property === 'italic' ? 'I' : 'U'}</button>)}
        </fieldset>
      </>}
      {(activeTab === 'write' || activeTab === 'layout') && <fieldset disabled={unavailable}
        className={`format-group alignment-controls${activeTab === 'write' ? ' advanced-formatting' : ''}`}>
          <legend className="visually-hidden">Paragraph alignment</legend>
          {(['left', 'center', 'right', 'justify'] as const).map((alignment) => <button key={alignment}
            className="format-button" aria-label={`Align ${alignment}`} aria-pressed={formatState?.alignment === alignment}
            onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'format_paragraph', alignment })}>
            <AlignmentIcon alignment={alignment} />
          </button>)}
      </fieldset>}
      {activeTab === 'write' && <button className="more-formatting" aria-label="More formatting" aria-expanded={moreFormatting}
        onClick={() => setMoreFormatting((value) => !value)}><WriterIcon name="more" /></button>}
      {activeTab === 'insert' && <div className="format-group insert-controls" role="group" aria-label="Insert text structure">
        <button disabled={unavailable} onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'insert_paragraph' })}>Paragraph <kbd>Enter</kbd></button>
        <button disabled={unavailable} onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'insert_line_break' })}>Line break <kbd>Shift Enter</kbd></button>
      </div>}
      {activeTab === 'layout' && <span className="formatting-note">Page sizes and margins follow the document.</span>}
      {activeTab === 'review' && <div className="format-group review-controls" role="group" aria-label="Review document">
        <button disabled={unavailable} onClick={openFind}><WriterIcon name="search" />Find</button>
        <button disabled={unavailable} aria-pressed={pagePreview} onClick={togglePagePreview}>Page preview</button>
      </div>}
    </div>
    {findOpen && view && documentOpen && <div ref={findPanel} className="find-panel"><FindBar view={view} onFind={findText} onCommand={command}
      onClose={() => setFindOpen(false)} onFocusDocument={() => surface.current?.focus()}
      onRevealSelection={(next) => surface.current?.revealSelection?.(next)} /></div>}
    {visibleError && !unsavedDialog && <div className="error-banner" role="alert"><span>{visibleError}</span>
      {saveConflict && <button onClick={() => void saveCopy()} disabled={busy}>Save a separate copy</button>}
      {saveState.error && <button disabled={busy || composing} onClick={() => void saveDocument()}>Retry save</button>}
      {error && <button aria-label="Dismiss error" onClick={() => setError('')}>×</button>}</div>}
    <main className="workspace" aria-label="Document workspace">
      {startupError ? <div className="startup-message" role="alert"><h1>Writer could not start</h1><p>{startupError}</p><button onClick={() => window.location.reload()}>Try again</button></div>
        : !view ? <p className="startup-message" role="status">Opening Writer…</p>
        : documentOpen ? <><div className="paged-editing-view" hidden={pagePreview}><PagedEditor ref={surface} view={view} onCommand={command}
          onError={setError} onCompositionChange={compositionChanged} disabled={opening || pagePreview}
          zoom={previewZoom} loadLayout={loadPageLayout} loadGeometry={loadSelectionGeometry}
          onInteraction={interact} onPageInfo={acceptEditingPageInfo}
          onCopy={() => {
            if (!engine.current) throw new Error('Writer is not ready to copy text.');
            return engine.current.selectedText();
          }} /></div>{pagePreview && <PagePreview key={JSON.stringify([view.sessionId, view.revision])}
            sessionId={view.sessionId} revision={view.revision} documentId={view.documentId}
            loadLayout={loadPageLayout} zoom={previewZoom} onZoomChange={setPreviewZoom}
            onPageInfo={acceptPreviewPageInfo}
            onClose={togglePagePreview} />}</>
          : <div className="closed-document"><WriterIcon name="lantern" className="writer-lantern" /><h1>Your document is closed</h1><p>Create a document or open one saved on this device.</p><div><button className="primary-button" onClick={newDocument}>New document</button><button onClick={() => void showLibrary()}>Open document</button></div></div>}
    </main>
    <footer className="status-bar">
      <span className="page-status" aria-live="polite">{!documentOpen ? 'No document open'
        : pageInfo.status === 'unavailable' ? 'Pages unavailable'
          : pageInfo.status === 'loading' ? 'Preparing pages…'
            : pageInfo.pageCount === 0 ? 'No pages'
              : `Page ${pageInfo.currentPage ?? '—'} of ${pageInfo.pageCount ?? '—'}`}</span>
      <span className="word-count">{documentOpen && view ? `${view.wordCount.toLocaleString()} ${view.wordCount === 1 ? 'word' : 'words'}` : 'Writer'}</span>
      <span role="status" className="status-message">{saveState.phase === 'error' ? saveLabel : notice || 'Your work stays in this browser · Export for a backup'}</span>
      <div className="zoom-controls" role="group" aria-label="Document zoom">
        {previewZoom === 'fit' && pageInfo.zoomPercent != null && documentOpen
          && <output className="zoom-percentage" aria-label="Zoom percentage">{pageInfo.zoomPercent}%</output>}
        <label className="visually-hidden" htmlFor="document-zoom">Zoom</label>
        <select id="document-zoom" aria-label="Zoom" value={previewZoom} disabled={!documentOpen}
          onChange={(event) => setPreviewZoom(event.target.value as PreviewZoom)}>
          <option value="fit">Fit width</option>{(['50', '75', '100', '125', '150', '200'] as const).map((level) => <option key={level} value={level}>{level}%</option>)}
        </select>
        <button aria-label="Zoom out" disabled={!documentOpen || previewZoom === '50'} onClick={() => changeZoom(-1)}><WriterIcon name="minus" /></button>
        <button aria-label="Zoom in" disabled={!documentOpen || previewZoom === '200'} onClick={() => changeZoom(1)}><WriterIcon name="plus" /></button>
      </div>
    </footer>
    <nav className="mobile-document-actions" aria-label="Mobile document actions">
      <button aria-label="New" disabled={!view || busy || composing} onClick={newDocument}><WriterIcon name="new" /><span>New</span></button>
      <button aria-label="Open" disabled={!view || busy || composing} onClick={() => void showLibrary()}><WriterIcon name="open" /><span>Open</span></button>
      <button aria-label="Find" disabled={unavailable} onClick={openFind}><WriterIcon name="search" /><span>Find</span></button>
      <button aria-label="Export" disabled={unavailable} onClick={exportFile}><WriterIcon name="export" /><span>Export</span></button>
    </nav>
    <input ref={fileInput} type="file" accept={`.writer,.docx,application/json,${DOCX_MEDIA_TYPE}`} className="visually-hidden" tabIndex={-1} aria-label="Import Writer file"
      onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ''; if (file) importFile(file); }} />
    {library !== null && <Dialog title="Open document" onClose={() => setLibrary(null)} busy={busy}>
      <p className="dialog-description">Documents saved in this browser on this device.</p>
      {libraryProblems.length > 0 && <div className="dialog-error" role="alert">
        <p>{libraryProblems.length} saved {libraryProblems.length === 1 ? 'document could' : 'documents could'} not be listed.
          Their data remains on this device. Other documents can still be opened.</p>
        {libraryProblems.map((problem, index) => <p key={index}>{problem.message}</p>)}
      </div>}
      <div className="document-library">{library.length === 0 ? <p>{libraryProblems.length ? 'No readable saved documents.' : 'No saved documents yet.'}</p> : library.map((entry) => <button className="library-entry" key={entry.id} disabled={busy}
        onClick={() => openSaved(entry.id)}><span>{entry.title || 'Untitled document'}</span><time dateTime={new Date(entry.updatedAt).toISOString()}>{new Date(entry.updatedAt).toLocaleString()}</time></button>)}</div>
      <p className="storage-note">Browser storage can be cleared or evicted. Keep exported .writer files as backups.</p>
      <div className="dialog-actions"><button disabled={busy} onClick={() => fileInput.current?.click()}>Import .writer or .docx file</button><button onClick={() => setLibrary(null)} disabled={busy}>Cancel</button></div>
    </Dialog>}
    {exportDisclosure && <Dialog title="Export Writer file" onClose={() => setExportDisclosure(false)}>
      <p>This Writer file includes the preserved original DOCX, {view?.originalSource?.filename}.</p>
      <p>The original may contain text or metadata removed from your edited document. Share a Word export when you need only current supported content.</p>
      {visibleError && <div className="dialog-error" role="alert">{visibleError}</div>}
      <div className="dialog-actions"><button onClick={() => setExportDisclosure(false)}>Cancel</button>
        <button className="primary-button" onClick={exportWriterFile}>Download Writer file</button></div>
    </Dialog>}
    {preservedImport && <Dialog title="DOCX preserved read-only" onClose={() => setPreservedImport(null)}>
      <p>{preservedImport.message}</p>
      <p>Your current document is unchanged. The original bytes remain available here for download. This read-only source has not been saved in the local document library.</p>
      <ul className="compatibility-issues">{preservedImport.report?.diagnostics.map((issue, index) =>
        <li key={index}><strong>{issue.part}: {issue.path}</strong><span>{issue.message}</span></li>)}</ul>
      {!!preservedImport.report?.omittedDiagnostics && <p>{preservedImport.report.omittedDiagnostics} additional issues were omitted from this bounded report.</p>}
      <div className="dialog-actions"><button onClick={() => download(preservedImport.file, preservedImport.file.name)}>Download original DOCX</button>
        <button className="primary-button" onClick={() => setPreservedImport(null)}>Close</button></div>
    </Dialog>}
    {unsavedDialog && <Dialog title="Save your changes?" onClose={() => { pending.current = null; setUnsavedDialog(false); }} busy={busy}>
      <p>Save this document before continuing. Unsaved changes will be lost if you discard them.</p>
      {visibleError && <div className="dialog-error" role="alert"><p>{visibleError}</p>
        {saveConflict && <button disabled={busy} onClick={() => void saveCopy().then((saved) => {
          if (saved) finishPending();
        })}>Save a separate copy</button>}
      </div>}
      <div className="dialog-actions">
        <button onClick={() => { pending.current = null; setUnsavedDialog(false); }} disabled={busy}>Cancel</button>
        <button disabled={busy} onClick={() => { saveController.current?.cancelPending(); finishPending(); }}>Discard changes</button>
        <button className="primary-button" disabled={busy} onClick={() => void saveDocument().then((saved) => {
          if (saved) finishPending();
        })}>Save and continue</button>
      </div>
    </Dialog>}
  </div>;
}
