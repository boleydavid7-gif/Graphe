import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { EngineClient } from './engine';
import { EditorSurface, type EditorSurfaceHandle } from './EditorSurface';
import { StorageError, WriterStorage, type StoredDocument } from './storage';
import type { Alignment, WriterCommand, WriterView } from './types';
import './styles.css';

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

export default function App() {
  const engine = useRef<EngineClient | null>(null);
  const storage = useRef<WriterStorage | null>(null);
  const surface = useRef<EditorSurfaceHandle>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const viewRef = useRef<WriterView | null>(null);
  const dirtyRef = useRef(false);
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
      if (!engine.current) return;
      const origin = viewRef.current;
      setBusy(true);
      setOpening(true);
      try {
        if (file.size > 5 * 1024 * 1024) throw new Error('This prototype opens Writer files up to 5 MiB.');
        const bytes = new Uint8Array(await file.arrayBuffer());
        if (viewRef.current?.revision !== origin?.revision || viewRef.current?.sessionId !== origin?.sessionId) {
          throw new Error('Your document changed while importing. Try again after saving your changes.');
        }
        const next = engine.current.open(bytes);
        replaceSession(next, null, 'Imported Writer file. Save a local copy to keep it on this device.');
      } catch (failure) { setError(`Could not open this file. ${message(failure)}`); }
      finally { setBusy(false); setOpening(false); }
    });
  }

  function exportFile() {
    if (!engine.current || !viewRef.current || !documentOpen || composing) return;
    try {
      const bytes = engine.current.serialize();
      const url = URL.createObjectURL(new Blob([bytes.slice().buffer], { type: 'application/json;charset=utf-8' }));
      const link = document.createElement('a');
      link.href = url;
      const filename = [...(viewRef.current.title.trim() || 'Untitled document')]
        .map((character) => character.charCodeAt(0) < 32 || '\\/:*?"<>|'.includes(character) ? '_' : character).join('').slice(0, 100);
      link.download = `${filename}.writer`;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      setNotice('Writer file exported. Export does not replace your local save.');
    } catch (failure) { setError(message(failure)); }
  }

  function closeDocument() {
    guardReplacement(() => {
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
        : documentOpen ? <div className="document-page"><EditorSurface ref={surface} view={view} onCommand={command}
          onError={setError} onCompositionChange={setComposing} disabled={opening}
          onCopy={() => {
            if (!engine.current) throw new Error('Writer is not ready to copy text.');
            return engine.current.selectedText();
          }} /></div>
          : <div className="closed-document"><span className="writer-mark">W</span><h1>Your document is closed</h1><p>Create a document or open one saved on this device.</p><div><button className="primary-button" onClick={newDocument}>New document</button><button onClick={() => void showLibrary()}>Open document</button></div></div>}
    </main>
    <footer className="status-bar"><span>{documentOpen && view ? `${view.wordCount.toLocaleString()} ${view.wordCount === 1 ? 'word' : 'words'}` : 'Writer'}</span>
      <span role="status" className="status-message">{notice || 'Your work stays in this browser. Export a file for a backup.'}</span>
      <span className="privacy-badge">Local document</span></footer>
    <input ref={fileInput} type="file" accept=".writer,application/json" className="visually-hidden" tabIndex={-1} aria-label="Import Writer file"
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
      <div className="dialog-actions"><button disabled={busy} onClick={() => fileInput.current?.click()}>Import .writer file</button><button onClick={() => setLibrary(null)} disabled={busy}>Cancel</button></div>
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
