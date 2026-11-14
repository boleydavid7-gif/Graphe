import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { EngineClient, EngineError } from './engine';
import { EditorSurface, type EditorSurfaceHandle } from './EditorSurface';
import { PagePreview, type LayoutOrigin, type PreviewZoom } from './PagePreview';
import { StorageError, WriterStorage, type StoredDocument } from './storage';
import type { Alignment, CompatibilityReport, WriterCommand, WriterView } from './types';
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
type Association = { sessionId: string; id: string; etag: string };
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
  const viewRef = useRef<WriterView | null>(null);
  const dirtyRef = useRef(false);
  const documentOpenRef = useRef(true);
  const association = useRef<Association | null>(null);
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
  const [composing, setComposing] = useState(false);
  const [library, setLibrary] = useState<readonly LibraryEntry[] | null>(null);
  const [libraryProblems, setLibraryProblems] = useState<readonly { message: string; id?: string }[]>([]);
  const [unsavedDialog, setUnsavedDialog] = useState(false);
  const [saveConflict, setSaveConflict] = useState(false);
  const [exportDisclosure, setExportDisclosure] = useState(false);
  const [preservedImport, setPreservedImport] = useState<PreservedImport | null>(null);
  const [pagePreview, setPagePreview] = useState(false);
  const [previewZoom, setPreviewZoom] = useState<PreviewZoom>('fit');
  const previousPreview = useRef(false);

  useLayoutEffect(() => {
    const enteringPreview = !previousPreview.current && pagePreview;
    const returningToEditing = previousPreview.current && !pagePreview;
    previousPreview.current = pagePreview;
    if (!documentOpen || !documentOpenRef.current) return;
    if (enteringPreview) {
      // This is a fixed interface root, independent of native document IDs.
      document.getElementById('writer-page-preview')?.focus({ preventScroll: true });
    } else if (returningToEditing) {
      // Restore the Rust selection in the same commit that reveals the editor.
      // Deferring focus would let the first keyboard event target a removed
      // preview control, or steal focus from a control reached afterward.
      surface.current?.focus();
    }
  }, [documentOpen, pagePreview]);

  const loadPageLayout = useCallback((origin: LayoutOrigin) => {
    const client = engine.current;
    if (!client || !documentOpenRef.current || client.view.sessionId !== origin.sessionId
      || client.view.revision !== origin.revision || client.view.documentId !== origin.documentId) {
      throw new Error('The document changed while preparing page preview. Return to editing and try again.');
    }
    return client.layout();
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
    if (trackChange && viewRef.current?.revision !== next.revision) setDirty(true);
    viewRef.current = next;
    setView(next);
    return next;
  }

  useEffect(() => {
    let canceled = false;
    let client: EngineClient | null = null;
    const localStore = new WriterStorage();
    storage.current = localStore;
    void EngineClient.create().then((created) => {
      client = created;
      if (canceled) { created.dispose(); return; }
      engine.current = created;
      viewRef.current = created.view;
      setView(created.view);
    }).catch((failure: unknown) => {
      if (!canceled) setStartupError(message(failure));
    });
    return () => {
      canceled = true;
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
    return () => window.removeEventListener('beforeunload', beforeUnload);
  }, []);

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

  function replaceSession(next: WriterView, source: Association | null, info: string) {
    const previousFocus = document.activeElement;
    association.current = source;
    setSavedLocally(source !== null);
    updateView(next, false);
    setDirty(false);
    documentOpenRef.current = true;
    setDocumentOpen(true);
    setSaveConflict(false);
    setError('');
    setNotice(info);
    setLibrary(null);
    requestAnimationFrame(() => {
      if (document.activeElement === previousFocus || document.activeElement === document.body) surface.current?.focus();
    });
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
    const client = engine.current;
    const local = storage.current;
    const captured = viewRef.current;
    if (!client || !local || !captured || !documentOpen || composing || busy) return false;
    setBusy(true);
    setError('');
    try {
      const bytes = client.serialize();
      const source = association.current;
      const expected = source?.sessionId === captured.sessionId && source.id === captured.documentId ? source.etag : null;
      const saved = await local.save({ id: captured.documentId, title: captured.title, bytes }, expected);
      const current = viewRef.current;
      if (current?.sessionId === captured.sessionId) {
        association.current = { sessionId: captured.sessionId, id: saved.id, etag: saved.etag };
        setSavedLocally(true);
        setSaveConflict(false);
        if (current.revision === captured.revision) {
          setDirty(false);
          setNotice('Saved on this device');
          return true;
        }
        setNotice('Earlier version saved. Save again to include your latest changes.');
      }
      return false;
    } catch (failure) {
      setSaveConflict(failure instanceof StorageError && failure.code === 'conflict');
      setError(`${message(failure)} Your document is still open and unchanged. You can also export a Writer file.`);
      return false;
    } finally { setBusy(false); }
  }

  async function saveCopy(): Promise<boolean> {
    if (!engine.current || busy || composing) return false;
    try {
      const next = engine.current.command({ type: 'fork_document' });
      association.current = null;
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
      documentOpenRef.current = false;
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
  return <div className="writer-app">
    <header className="top-bar">
      <div className="identity" role="img" aria-label="Writer"><span className="writer-mark">W</span><span>Writer</span></div>
      <div className="title-area">
        <input aria-label="Document title" className="document-title" placeholder="Untitled document"
          value={documentOpen ? view?.title ?? '' : ''} disabled={unavailable} maxLength={250}
          onChange={(event) => command({ type: 'set_title', title: event.target.value })} />
        <span className="save-state" aria-live="polite">{!documentOpen ? 'No document open' : dirty ? 'Unsaved changes' : savedLocally ? 'Saved on this device' : 'Not saved yet'}</span>
      </div>
      <nav className="file-actions" aria-label="Document actions">
        <button disabled={!view || busy || composing} onClick={newDocument}>New</button>
        <button disabled={!view || busy || composing} onClick={() => void showLibrary()}>Open</button>
        <button className="primary-button" disabled={unavailable} onClick={() => void saveDocument()}>{busy ? 'Working…' : 'Save'}</button>
        <span className="action-divider" />
        <button aria-label="Undo" title="Undo (Ctrl/⌘ Z)" disabled={unavailable || !view?.canUndo}
          onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'undo' })}>↶</button>
        <button aria-label="Redo" title="Redo (Ctrl/⌘ Shift Z)" disabled={unavailable || !view?.canRedo}
          onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'redo' })}>↷</button>
        <button className="secondary-file-action" disabled={unavailable} onClick={exportFile}>Export</button>
        <button className="secondary-file-action" disabled={unavailable} onClick={exportWordFile}>Export Word</button>
        <button className="secondary-file-action" disabled={unavailable} onMouseDown={(event) => event.preventDefault()}
          onClick={togglePagePreview} aria-pressed={pagePreview} aria-controls="writer-page-preview">Page preview</button>
        {view?.originalSource && <button className="secondary-file-action" disabled={unavailable} onClick={downloadOriginal}>Download original</button>}
        <button className="secondary-file-action" disabled={unavailable} onClick={closeDocument}>Close</button>
      </nav>
    </header>
    <div className="ribbon" aria-label="Home formatting">
      <div className="ribbon-tab">Home</div>
      <div className="ribbon-controls">
        <fieldset disabled={unavailable} className="ribbon-group font-group">
          <legend>Font</legend>
          <label className="visually-hidden" htmlFor="font-family">Font family</label>
          <select id="font-family" aria-label="Font family" value={formatState?.fontFamily ?? ''}
            onChange={(event) => format({ type: 'format_character', fontFamily: event.target.value }, false)}>
            <option value="" disabled>Mixed</option>
            {[...new Set(['Arial', 'Georgia', 'Times New Roman', 'Verdana', 'Courier New', ...(formatState?.fontFamily ? [formatState.fontFamily] : [])])].map((font) => <option key={font}>{font}</option>)}
          </select>
          <label className="visually-hidden" htmlFor="font-size">Font size</label>
          <select id="font-size" aria-label="Font size" value={formatState?.fontSizePt ?? ''}
            onChange={(event) => format({ type: 'format_character', fontSizePt: Number(event.target.value) }, false)}>
            <option value="" disabled>—</option>
            {[...new Set([8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72, ...(formatState?.fontSizePt ? [formatState.fontSizePt] : [])])].sort((a, b) => a - b).map((size) => <option key={size} value={size}>{size}</option>)}
          </select>
          {(['bold', 'italic', 'underline'] as const).map((property) => <button key={property}
            className={`format-button format-${property}`} aria-label={property.charAt(0).toUpperCase() + property.slice(1)}
            aria-pressed={formatState?.[property] === null ? 'mixed' : !!formatState?.[property]}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => format({ type: 'format_character', [property]: !formatState?.[property] })}>{property === 'bold' ? 'B' : property === 'italic' ? 'I' : 'U'}</button>)}
        </fieldset>
        <fieldset disabled={unavailable} className="ribbon-group">
          <legend>Paragraph</legend>
          {(['left', 'center', 'right', 'justify'] as const).map((alignment) => <button key={alignment}
            className="format-button" aria-label={`Align ${alignment}`} aria-pressed={formatState?.alignment === alignment}
            onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'format_paragraph', alignment })}>
            <AlignmentIcon alignment={alignment} />
          </button>)}
        </fieldset>
        <span className="ribbon-hint">Native Writer document · Private prototype</span>
      </div>
    </div>
    {error && !unsavedDialog && <div className="error-banner" role="alert"><span>{error}</span>
      {saveConflict && <button onClick={() => void saveCopy()} disabled={busy}>Save a separate copy</button>}
      <button aria-label="Dismiss error" onClick={() => setError('')}>×</button></div>}
    <main className="workspace" aria-label="Document workspace">
      {startupError ? <div className="startup-message" role="alert"><h1>Writer could not start</h1><p>{startupError}</p><button onClick={() => window.location.reload()}>Try again</button></div>
        : !view ? <p className="startup-message" role="status">Opening Writer…</p>
        : documentOpen ? <><div className="document-page" hidden={pagePreview}><EditorSurface ref={surface} view={view} onCommand={command}
          onError={setError} onCompositionChange={setComposing} disabled={opening || pagePreview}
          onCopy={() => {
            if (!engine.current) throw new Error('Writer is not ready to copy text.');
            return engine.current.selectedText();
          }} /></div>{pagePreview && <PagePreview key={JSON.stringify([view.sessionId, view.revision])}
            sessionId={view.sessionId} revision={view.revision} documentId={view.documentId}
            loadLayout={loadPageLayout} zoom={previewZoom} onZoomChange={setPreviewZoom}
            onClose={togglePagePreview} />}</>
          : <div className="closed-document"><span className="writer-mark">W</span><h1>Your document is closed</h1><p>Create a document or open one saved on this device.</p><div><button className="primary-button" onClick={newDocument}>New document</button><button onClick={() => void showLibrary()}>Open document</button></div></div>}
    </main>
    <footer className="status-bar"><span>{documentOpen && view ? `${view.wordCount.toLocaleString()} ${view.wordCount === 1 ? 'word' : 'words'}` : 'Writer'}</span>
      <span role="status" className="status-message">{notice || 'Your work stays in this browser. Export a file for a backup.'}</span>
      <span className="privacy-badge">Local document</span>
      <a className="font-licenses" href={fontLicensesUrl} target="_blank" rel="noreferrer">Font licenses</a></footer>
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
      {error && <div className="dialog-error" role="alert">{error}</div>}
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
      {error && <div className="dialog-error" role="alert"><p>{error}</p>
        {saveConflict && <button disabled={busy} onClick={() => void saveCopy().then((saved) => {
          if (saved) finishPending();
        })}>Save a separate copy</button>}
      </div>}
      <div className="dialog-actions">
        <button onClick={() => { pending.current = null; setUnsavedDialog(false); }} disabled={busy}>Cancel</button>
        <button disabled={busy} onClick={finishPending}>Discard changes</button>
        <button className="primary-button" disabled={busy} onClick={() => void saveDocument().then((saved) => {
          if (saved) finishPending();
        })}>Save and continue</button>
      </div>
    </Dialog>}
  </div>;
}
