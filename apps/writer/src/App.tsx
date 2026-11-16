import { announceFilesReady, canSaveToProairetos, requestProairetosSaveDialog, sendProairetosExport, type FilesOpenPayload } from './proairetosFilesBridge';
import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { EngineClient, EngineError } from './engine';
import type { EditorSurfaceHandle } from './EditorSurface';
import { PagedEditor, type PageInfo } from './PagedEditor';
import { PagePreview, type LayoutOrigin, type PreviewZoom } from './PagePreview';
import { WriterIcon } from './WriterIcon';
import { FindBar } from './FindBar';
import { StorageError, WriterStorage, type StoredDocument } from './storage';
import { WriterSaveController, type SaveAssociation, type SaveState } from './autosave';
import type { Alignment, CompatibilityReport, DocumentFormat, ExportAssessment, LayoutAction, WriterCommand, WriterView } from './types';
import './styles.css';
import fontLicensesUrl from '../../../crates/office-layout/fonts/NOTICE.txt?url';
import formatLicensesUrl from '../../../crates/office-formats/THIRD_PARTY_NOTICES.txt?url';
import grapheLicenseUrl from '../../../LICENSE?url';

function safeHyperlink(value: string): boolean {
  if (value.length === 0 || value.length > 2048 ||
    /[\s\x00-\x1f\x7f<>"\\]/.test(value)) return false;
  try {
    const url = new URL(value);
    return (url.protocol === 'https:' || url.protocol === 'http:') &&
      !url.username && !url.password && !!url.hostname;
  } catch { return false; }
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : 'The operation could not be completed.';
}

function Dialog({ title, children, onClose, busy = false, className }: {
  title: string; children: ReactNode; onClose: () => void; busy?: boolean; className?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const label = useId();
  useLayoutEffect(() => {
    const dialog = ref.current;
    const visual = window.visualViewport;
    const position = () => {
      dialog?.style.setProperty('--save-copy-top', `${visual?.offsetTop ?? 0}px`);
      dialog?.style.setProperty('--save-copy-height', `${visual?.height ?? window.innerHeight}px`);
    };
    if (className?.split(' ').includes('save-copy-dialog')) {
      position();
      window.addEventListener('resize', position);
      visual?.addEventListener('resize', position);
      visual?.addEventListener('scroll', position);
    }
    dialog?.showModal();
    return () => {
      window.removeEventListener('resize', position);
      visual?.removeEventListener('resize', position);
      visual?.removeEventListener('scroll', position);
      dialog?.close();
    };
  }, [className]);
  return <dialog ref={ref} className={className} aria-labelledby={label} onCancel={(event) => {
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
type PreservedImport = { file: File; format: DocumentFormat; message: string; report: CompatibilityReport | undefined };
type HyperlinkDialogState = {
  label: string;
  initialLabel: string;
  url: string;
  existing: boolean;
  selection: WriterView['selection'];
  origin: Pick<WriterView, 'sessionId' | 'documentId' | 'revision'>;
};
const PARAGRAPH_METRICS = [
  { key: 'spacingBeforePt', label: 'Before', unit: 'pt', min: 0, max: 144, step: 0.5 },
  { key: 'spacingAfterPt', label: 'After', unit: 'pt', min: 0, max: 144, step: 0.5 },
  { key: 'indentLeftIn', label: 'Left', unit: 'in', min: 0, max: 4, step: 0.05 },
  { key: 'indentRightIn', label: 'Right', unit: 'in', min: 0, max: 4, step: 0.05 },
  { key: 'firstLineIn', label: 'First line', unit: 'in', min: -2, max: 4, step: 0.05 },
] as const;
type ParagraphMetricKey = (typeof PARAGRAPH_METRICS)[number]['key'] | 'lineSpacingMultiple';
type ParagraphMetricFields = Record<ParagraphMetricKey, string>;
type ParagraphDialogState = {
  values: ParagraphMetricFields;
  original: ParagraphMetricFields;
  selection: WriterView['selection'];
  origin: Pick<WriterView, 'sessionId' | 'documentId' | 'revision'>;
  error: string;
};

function numericParagraphValue(value: number | null): string {
  return value === null ? '' : String(Number(value.toFixed(3)));
}

// Names below are backed by the pinned Rust font catalog, not system fonts.
// Unknown names from imported documents remain selectable and are preserved.
const FONT_GROUPS = [
  { label: 'Sans serif', names: ['Arimo', 'Carlito', 'Source Sans 3', 'Inter', 'Montserrat', 'DM Sans'] },
  { label: 'Serif', names: ['Tinos', 'Caladea', 'EB Garamond', 'Libre Baskerville', 'Lora', 'Bitter'] },
  { label: 'Display', names: ['Playfair Display', 'Space Grotesk'] },
  { label: 'Monospace', names: ['JetBrains Mono'] },
  { label: 'Languages and symbols', names: [
    'Noto Sans Arabic', 'Noto Sans Devanagari', 'Noto Sans Hebrew', 'Noto Emoji',
  ] },
] as const;
const BUNDLED_FONTS = new Set<string>(FONT_GROUPS.flatMap((group) => [...group.names]));
function bundledFontPreview(font: string | null | undefined): string {
  switch (font?.toLowerCase()) {
    case 'arial': case 'helvetica': return 'Arimo';
    case 'times new roman': case 'times': case 'serif': return 'Tinos';
    case 'calibri': return 'Carlito';
    case 'cambria': return 'Caladea';
    case 'source sans pro': return 'Source Sans 3';
    default: return font && BUNDLED_FONTS.has(font) ? font : 'Arimo';
  }
}
function bundledFontSample(font: string | null | undefined): string {
  switch (font) {
    case 'Noto Sans Arabic': return 'مرحباً بالعالم';
    case 'Noto Sans Devanagari': return 'नमस्ते दुनिया';
    case 'Noto Sans Hebrew': return 'שלום עולם';
    case 'Noto Emoji': return '☀ ♥ ☺';
    default: return 'Aa Bb Cc 123';
  }
}
function fontOptions(current: string | null | undefined) {
  return <>
    <option value="" disabled>{current === null ? 'Mixed fonts' : 'Select font'}</option>
    {FONT_GROUPS.map((group) => <optgroup key={group.label} label={group.label}>
      {group.names.map((name) => <option key={name} value={name}
        style={{ fontFamily: `"${name}", sans-serif` }}>{name}</option>)}
    </optgroup>)}
    {current && !BUNDLED_FONTS.has(current) &&
      <optgroup label="From document (using a substitute)">
        <option value={current}>{current}</option>
      </optgroup>}
  </>;
}

type SaveAsState = {
  format: DocumentFormat;
  filename: string;
  assessment: ExportAssessment | null;
  error: string;
};

const DOCX_MEDIA_TYPE = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
const FILE_FORMATS: readonly { format: DocumentFormat; label: string; mediaType: string }[] = [
  { format: 'writer', label: 'Graphe document (.writer)', mediaType: 'application/octet-stream' },
  { format: 'docx', label: 'Word document (.docx)', mediaType: DOCX_MEDIA_TYPE },
  { format: 'odt', label: 'OpenDocument text (.odt)', mediaType: 'application/vnd.oasis.opendocument.text' },
  { format: 'rtf', label: 'Rich Text Format (.rtf)', mediaType: 'application/rtf' },
  { format: 'txt', label: 'Plain text (.txt)', mediaType: 'text/plain;charset=utf-8' },
  { format: 'pdf', label: 'PDF document (.pdf) — export only', mediaType: 'application/pdf' },
];

function fileFormat(file: File): DocumentFormat {
  const extension = file.name.split('.').at(-1)?.toLowerCase();
  const byExtension = FILE_FORMATS.find((item) => item.format === extension);
  if (byExtension) return byExtension.format;
  const mediaType = file.type.toLowerCase().split(';')[0];
  // Generic binary MIME is deliberately not used to guess Writer on import.
  const byMediaType = FILE_FORMATS.find((item) => item.format !== 'writer' && item.mediaType.split(';')[0] === mediaType);
  if (byMediaType) return byMediaType.format;
  if (mediaType === 'text/rtf') return 'rtf';
  throw new Error('Choose a .writer, .docx, .odt, .rtf or .txt file. TXT supports UTF-8 or BOM-marked UTF-16. PDF is available for export only.');
}

function formatName(format: string): string {
  return format === 'writer' ? 'Graphe' : format.toUpperCase();
}

function formatMediaType(format: string): string {
  return FILE_FORMATS.find((item) => item.format === format)?.mediaType ?? 'application/octet-stream';
}

function requestDownload(url: string, filename: string) {
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.target = '_blank';
  link.rel = 'noopener';
  link.hidden = true;
  // Keep the activation target connected and inside the active modal, rather
  // than inert behind it. A browser that previews a file must not replace Writer.
  (document.activeElement?.closest('dialog[open]') ?? document.body).append(link);
  try { link.click(); }
  finally { link.remove(); }
}

function safeFilename(title: string, extension: string): string {
  const base = (title.trim() || 'Untitled document').replace(/\.(writer\.json|writer|docx|odt|rtf|txt|pdf)$/i, '');
  const name = [...(base || 'Untitled document')]
    .map((character) => {
      const code = character.codePointAt(0) ?? 0;
      return code < 32 || (code >= 0x7f && code <= 0x9f) || '\\/:*?"<>|'.includes(character) ? '_' : character;
    }).slice(0, 100).join('');
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
  const saveAsReturnFocus = useRef<{
    target: HTMLElement | null; editor: boolean; sessionId: string; documentId: string;
  } | null>(null);
  const pendingSaveAsFocus = useRef<{
    previous: Element | null;
    requested: NonNullable<typeof saveAsReturnFocus.current>;
  } | null>(null);
  const viewRef = useRef<WriterView | null>(null);
  const dirtyRef = useRef(false);
  const documentOpenRef = useRef(true);
  const saveController = useRef<WriterSaveController | null>(null);
  const pending = useRef<(() => void | Promise<void>) | null>(null);
  const preparedExportUrl = useRef<string | null>(null);
  const [preparedExport, setPreparedExport] = useState<{ url: string; filename: string; automaticFailed: boolean } | null>(null);
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
  const [aboutOpen, setAboutOpen] = useState(false);
  const [saveAs, setSaveAs] = useState<SaveAsState | null>(null);
  const [preservedImport, setPreservedImport] = useState<PreservedImport | null>(null);
  const [pagePreview, setPagePreview] = useState(false);
  const [previewZoom, setPreviewZoom] = useState<PreviewZoom>('fit');
  const [moreFormatting, setMoreFormatting] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [hyperlinkDialog, setHyperlinkDialog] = useState<HyperlinkDialogState | null>(null);
  const [paragraphDialog, setParagraphDialog] = useState<ParagraphDialogState | null>(null);
  const [activeTab, setActiveTab] = useState<'write' | 'insert' | 'layout' | 'review'>('write');
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const ribbonRef = useRef<HTMLDivElement | null>(null);
  const previousTabRef = useRef(activeTab);
  useEffect(() => {
    if (previousTabRef.current === activeTab) return;
    previousTabRef.current = activeTab;
    const ribbon = ribbonRef.current;
    if (!ribbon || typeof ribbon.animate !== 'function' ||
      window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const animation = ribbon.animate(
      [{ opacity: .68, transform: 'translateY(3px)' }, { opacity: 1, transform: 'translateY(0)' }],
      { duration: 230, easing: 'cubic-bezier(.22, 1, .36, 1)' }
    );
    return () => animation.cancel();
  }, [activeTab]);

  const [pageInfo, setPageInfo] = useState<PageInfo>({ pageCount: null, currentPage: null, status: 'loading', zoomPercent: null });
  const editingPageInfo = useRef(pageInfo);
  const previousPreview = useRef(false);

  useEffect(() => () => {
    if (preparedExportUrl.current) URL.revokeObjectURL(preparedExportUrl.current);
  }, []);

  function dismissPreparedExport() {
    if (preparedExportUrl.current) URL.revokeObjectURL(preparedExportUrl.current);
    preparedExportUrl.current = null;
    setPreparedExport(null);
  }

  function download(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob);
    const previous = preparedExportUrl.current;
    preparedExportUrl.current = url;
    let automaticFailed = false;
    try { requestDownload(url, filename); }
    catch { automaticFailed = true; }
    setPreparedExport({ url, filename, automaticFailed });
    if (previous) URL.revokeObjectURL(previous);
    // Keep this one immutable copy usable through a fresh, native link gesture.
    // There is no browser API confirming that a download reached Files/disk.
  }

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

  useLayoutEffect(() => {
    const pendingFocus = pendingSaveAsFocus.current;
    if (saveAs || !pendingFocus) return;
    pendingSaveAsFocus.current = null;
    // Restore during the close commit; a delayed callback could steal a newer
    // title/Find focus or lose the first key after dismissing the dialog.
    if (document.activeElement !== pendingFocus.previous && document.activeElement !== document.body) return;
    const { requested } = pendingFocus;
    const current = viewRef.current;
    // Closed details can retain nonzero client rectangles for their hidden
    // contents. A menu invocation must return to its visible summary.
    const target = requested.target && documentMenu.current?.contains(requested.target)
      ? documentMenu.current.querySelector('summary') : requested.target;
    if (requested.editor && current?.sessionId === requested.sessionId
      && current.documentId === requested.documentId && documentOpenRef.current && !previousPreview.current) {
      surface.current?.focus();
    } else if (!requested.editor && target?.isConnected && target.getClientRects().length) {
      target.focus({ preventScroll: true });
    } else {
      documentMenu.current?.querySelector('summary')?.focus({ preventScroll: true });
    }
  }, [saveAs]);

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

  const currentEngineView = useCallback(() => documentOpenRef.current ? engine.current?.view ?? null : null, []);

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
    const menu = documentMenu.current;
    const panel = menu?.querySelector<HTMLElement>('.document-menu-panel');
    const visual = window.visualViewport;
    function closeMenu(event: PointerEvent) {
      if (event.target instanceof Node && !documentMenu.current?.contains(event.target)) {
        documentMenu.current?.removeAttribute('open');
      }
    }
    function fitMenu() {
      if (!menu?.open || !panel) return;
      const anchor = menu.getBoundingClientRect();
      const visibleTop = visual?.offsetTop ?? 0;
      const bottom = visibleTop + (visual?.height ?? window.innerHeight);
      const top = Math.max(anchor.bottom + 8, visibleTop + 8);
      panel.style.top = `${top - anchor.top}px`;
      panel.style.setProperty('--document-menu-height', `${Math.max(0, bottom - top - 8)}px`);
    }
    document.addEventListener('pointerdown', closeMenu);
    menu?.addEventListener('toggle', fitMenu);
    window.addEventListener('resize', fitMenu);
    visual?.addEventListener('resize', fitMenu);
    visual?.addEventListener('scroll', fitMenu);
    return () => {
      document.removeEventListener('pointerdown', closeMenu);
      menu?.removeEventListener('toggle', fitMenu);
      window.removeEventListener('resize', fitMenu);
      visual?.removeEventListener('resize', fitMenu);
      visual?.removeEventListener('scroll', fitMenu);
    };
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
    // Native dialogs must return to a visible opener, not a now-hidden item.
    documentMenu.current?.querySelector('summary')?.focus({ preventScroll: true });
    action();
  }

  function openHyperlink() {
    const current = viewRef.current;
    if (!documentOpenRef.current || !current || !engine.current || composing || busy) return;
    try {
      const label = engine.current.selectedText();
      setHyperlinkDialog({
        label, initialLabel: label, url: current.format.hyperlink ?? 'https://',
        existing: !!current.format.hyperlink, selection: current.selection,
        origin: { sessionId: current.sessionId, documentId: current.documentId,
          revision: current.revision },
      });
      setError('');
    } catch (failure) {
      setError(message(failure));
    }
  }

  function submitHyperlink(remove = false) {
    const state = hyperlinkDialog;
    const current = viewRef.current;
    if (!state || !current || !engine.current || busy || composing) return;
    if (state.origin.sessionId !== current.sessionId ||
      state.origin.documentId !== current.documentId ||
      state.origin.revision !== current.revision) {
      setError('The document changed while editing the link. Close and reopen this dialog.');
      return;
    }
    const url = remove ? null : state.url.trim();
    if (!remove && !safeHyperlink(url ?? '')) {
      setError('Enter a full http:// or https:// address without spaces or credentials.');
      return;
    }
    const newText = !remove && (state.label !== state.initialLabel ||
      (!state.initialLabel && !state.existing)) ? state.label : undefined;
    if (newText !== undefined && (!newText || newText.length > 2048 ||
      /[\r\n]/.test(newText))) {
      setError('Link text must be a single line of at most 2,048 characters.');
      return;
    }
    const result = command({ type: 'set_hyperlink', url, text: newText,
      selection: state.selection ?? undefined, expectedRevision: state.origin.revision });
    if (result) setHyperlinkDialog(null);
  }

  function openParagraphDialog() {
    const current = viewRef.current;
    if (!current || !documentOpenRef.current || busy || composing) return;
    const format = current.format;
    const fields: ParagraphMetricFields = {
      spacingBeforePt: numericParagraphValue(format.spacingBeforePt),
      spacingAfterPt: numericParagraphValue(format.spacingAfterPt),
      lineSpacingMultiple: numericParagraphValue(format.lineSpacingMultiple),
      indentLeftIn: numericParagraphValue(format.indentLeftIn),
      indentRightIn: numericParagraphValue(format.indentRightIn),
      firstLineIn: numericParagraphValue(format.firstLineIn),
    };
    setParagraphDialog({
      values: { ...fields },
      original: fields,
      selection: current.selection,
      origin: { sessionId: current.sessionId, documentId: current.documentId,
        revision: current.revision },
      error: '',
    });
  }

  function submitParagraphMetrics() {
    const state = paragraphDialog;
    const current = viewRef.current;
    if (!state || !current || busy || composing) return;
    if (current.sessionId !== state.origin.sessionId ||
      current.documentId !== state.origin.documentId ||
      current.revision !== state.origin.revision) {
      setParagraphDialog({ ...state, error: 'The document changed. Close this dialog and try again.' });
      return;
    }
    const edits: Partial<Record<ParagraphMetricKey, number>> = {};
    const limits = [
      ...PARAGRAPH_METRICS,
      { key: 'lineSpacingMultiple' as const, label: 'Line spacing', unit: '', min: 1, max: 3, step: 0.05 },
    ];
    for (const field of limits) {
      const raw = state.values[field.key];
      if (raw === state.original[field.key]) continue;
      const numeric = raw.trim() === '' ? NaN : Number(raw);
      if (!Number.isFinite(numeric) || numeric < field.min || numeric > field.max) {
        setParagraphDialog({ ...state,
          error: `${field.label} must be between ${field.min} and ${field.max}${field.unit ? ' ' + field.unit : ''}.` });
        return;
      }
      edits[field.key] = numeric;
    }
    if (Object.keys(edits).length === 0) {
      setParagraphDialog(null);
      return;
    }
    const result = command({ type: 'set_paragraph_metrics',
      ...edits, selection: state.selection ?? undefined,
      expectedRevision: state.origin.revision });
    if (result) {
      setParagraphDialog(null);
    } else {
      setParagraphDialog({ ...state, error: 'The paragraph change was not applied. Check the document error and try again.' });
    }
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
    // Complete native return focus before capturing the replacement's origin.
    // Busy state may already have blurred a disabled dialog button; close owned
    // stacked dialogs from the top, rather than relying on the active element.
    const dialogs = documentMenu.current?.closest('.writer-app')?.querySelectorAll<HTMLDialogElement>('dialog[open]');
    Array.from(dialogs ?? []).reverse().forEach((dialog) => dialog.close());
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

  function importFile(file: File, filesId?: string) {
    guardReplacement(async () => {
      const active = engine.current;
      const local = storage.current;
      if (!active) return;
      const origin = viewRef.current;
      const wasOpen = documentOpenRef.current;
      let selectedFormat: DocumentFormat | undefined;
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
        selectedFormat = fileFormat(file);
        if (selectedFormat === 'pdf') throw new Error('PDF is export only. Open the editable Writer, DOCX, ODT, RTF or TXT source instead.');
        if (file.size > 5 * 1024 * 1024) throw new Error('Writer opens files up to 5 MiB, including retained originals.');
        const bytes = new Uint8Array(await file.arrayBuffer());
        checkOrigin();
        if (selectedFormat !== 'writer') {
          if (!local) throw new Error('Local storage is required to preserve the imported document before editing.');
          candidate = await EngineClient.create();
          checkOrigin();
          const next = candidate.importFormat(selectedFormat, bytes, file.name);
          const native = candidate.serialize();
          // A supported import is not activated until the native document and
          // exact original have completed one existing IndexedDB transaction.
          const saved = await local.save({ id: next.documentId, title: next.title, bytes: native }, null);
          checkOrigin();
          engine.current = candidate;
          candidate = null;
          replaceSession(next, { sessionId: next.sessionId, id: saved.id, etag: saved.etag },
            `${formatName(selectedFormat)} imported. ${next.originalSource ? 'The document and its exact original are' : 'The document is'} saved on this device.`);
          if (filesId) { setProairetosFileId(filesId); proairetosSession.current = next.sessionId; }
          active.dispose();
        } else {
          const next = active.open(bytes);
          replaceSession(next, null, 'Imported Writer file. Save a local copy to keep it on this device.');
          if (filesId) { setProairetosFileId(filesId); proairetosSession.current = next.sessionId; }
        }
      } catch (failure) {
        if (selectedFormat && selectedFormat !== 'writer' && failure instanceof EngineError
          && (failure.compatibility || failure.code === 'unsupported_docx')) {
          setPreservedImport({ file, format: selectedFormat, message: failure.message, report: failure.compatibility });
          setLibrary(null);
        } else setError(`Could not open this file. ${message(failure)}`);
      } finally {
        candidate?.dispose();
        setBusy(false);
        setOpening(false);
      }
    });
  }

  const importFileRef = useRef(importFile);
  importFileRef.current = importFile;
  useEffect(() => {
    const open = (event: Event) => {
      const payload = (event as CustomEvent<FilesOpenPayload>).detail;
      if (!payload) return;
      importFileRef.current(new File([payload.data], payload.name, { type: payload.mime }), payload.fileId);
    };
    window.addEventListener('proairetos:files-open', open);
    announceFilesReady();
    return () => window.removeEventListener('proairetos:files-open', open);
  }, []);

  useEffect(() => {
    const exportRequest = (event: MessageEvent) => {
      if (event.source !== window.parent || !canSaveToProairetos()
        || !['https://proairetos.com','https://research-and-development-proairetos.boleydavid7.workers.dev'].includes(event.origin)
        || event.data?.type !== 'proairetos:files-export-request' || event.data.version !== 1) return;
      const {requestId,name,format} = event.data;
      if (typeof requestId !== 'string' || !/^[a-zA-Z0-9_-]{8,80}$/.test(requestId)
        || typeof name !== 'string' || name.length > 120
        || !['writer','docx','odt','rtf','txt','pdf'].includes(format)
        || !name.toLowerCase().endsWith('.'+format)) return;
      try {
        if (!engine.current) throw new Error('Writer is not ready.');
        const assessment=engine.current.assessExport(format as DocumentFormat);
        const bytes=engine.current.exportFormat(format as DocumentFormat,assessment.warnings.length>0,assessment);
        const session=viewRef.current?.sessionId;
        const fileId=proairetosSession.current === session ? proairetosFileId : null;
        sendProairetosExport(requestId,name,format,bytes,fileId ?? undefined);
      } catch (failure) { setError(message(failure)); }
    };
    window.addEventListener('message',exportRequest);
    return () => window.removeEventListener('message',exportRequest);
  });

  function exportWriterFile() {
    if (!engine.current || !viewRef.current || !documentOpen || composing) return;
    try {
      const bytes = engine.current.serialize();
      download(new Blob([bytes.slice().buffer], { type: formatMediaType('writer') }), safeFilename(viewRef.current.title, 'writer'));
      setExportDisclosure(false);
      setNotice('Graphe copy ready. Export does not replace your local save.');
    } catch (failure) { setError(message(failure)); }
  }

  function exportFile() {
    if (viewRef.current?.originalSource) {
      setError('');
      setExportDisclosure(true);
    }
    else exportWriterFile();
  }

  function reviewSaveAs(selectedFormat?: DocumentFormat, filename?: string) {
    const client = engine.current;
    const current = viewRef.current;
    if (!client || !current || !documentOpen || composing || busy) return;
    const sourceFormat = current.originalSource?.format;
    const format = selectedFormat ?? FILE_FORMATS.find((item) => item.format === sourceFormat)?.format ?? 'writer';
    let assessment: ExportAssessment | null = null;
    let failureMessage = '';
    try { assessment = client.assessExport(format); }
    catch (failure) { failureMessage = message(failure); }
    if (canSaveToProairetos()) {
      const warnings: Record<string,string[]> = {};
      for (const item of FILE_FORMATS) {
        try { warnings[item.format] = client.assessExport(item.format).warnings.map((warning) => warning.message); }
        catch (failure) { warnings[item.format] = [message(failure)]; }
      }
      const sourceId = proairetosSession.current === current.sessionId ? proairetosFileId : null;
      void requestProairetosSaveDialog(filename ?? safeFilename(current.title,format),format,warnings,sourceId ?? undefined)
        .then((id)=>{ if (viewRef.current?.sessionId === current.sessionId) {setProairetosFileId(id);proairetosSession.current=current.sessionId;} setNotice('Saved in ProairetOS Files.'); })
        .catch((failure)=>{ if (message(failure) !== 'Save cancelled.') setError(message(failure)); });
      return;
    }
    rememberSaveAsFocus();
    setError('');
    setSaveAs({ format, filename: filename ?? safeFilename(current.title, format), assessment, error: failureMessage });
  }

  function rememberSaveAsFocus() {
    if (saveAs || !viewRef.current) return;
    const active = document.activeElement;
    const target = active instanceof HTMLElement && active !== document.body && active.getClientRects().length
      && !documentMenu.current?.contains(active)
      ? active : documentMenu.current?.querySelector('summary') ?? null;
    saveAsReturnFocus.current = { target, editor: target?.getAttribute('data-testid') === 'editor',
      sessionId: viewRef.current.sessionId, documentId: viewRef.current.documentId };
  }

  function closeSaveAs() {
    // This is a fixed interface ID, independent of native document identifiers.
    // Close before unmount so the native dialog's later cleanup cannot replace
    // the deliberate focus restoration with its now-hidden menu trigger.
    document.getElementById('save-as-format')?.closest('dialog')?.close();
    if (saveAsReturnFocus.current) {
      pendingSaveAsFocus.current = { previous: document.activeElement, requested: saveAsReturnFocus.current };
      saveAsReturnFocus.current = null;
    }
    setSaveAs(null);
  }

  function changeSaveAsFormat(format: DocumentFormat) {
    const filename = safeFilename((saveAs?.filename ?? viewRef.current?.title ?? '')
      .replace(/\.(writer|docx|odt|rtf|txt|pdf)$/i, ''), format);
    reviewSaveAs(format, filename);
  }

  const [proairetosFileId, setProairetosFileId] = useState<string | null>(null);
  const proairetosSession = useRef<string | null>(null);

  function downloadSaveAs() {
    const current = saveAs;
    if (!engine.current || !current?.assessment || !documentOpen || composing || busy) return;
    try {
      // Save copy confirms the brief warning already shown for this revision.
      // A download never changes the native local save/etag association.
      const bytes = engine.current.exportFormat(current.format, current.assessment.warnings.length > 0, current.assessment);
      const filename = safeFilename(current.filename.replace(/\.(writer|docx|odt|rtf|txt|pdf)$/i, ''), current.format);
      download(new Blob([bytes.slice().buffer], { type: formatMediaType(current.format) }), filename);
      closeSaveAs();
      setNotice(`${formatName(current.format)} copy ready. This does not replace your local Writer save.`);
    } catch (failure) {
      setSaveAs({ ...current, assessment: null, error: message(failure) });
    }
  }

  function exportWordFile() {
    if (!engine.current || !viewRef.current || !documentOpen || composing) return;
    try {
      const assessment = engine.current.assessExport('docx');
      if (assessment.warnings.length) {
        rememberSaveAsFocus();
        setSaveAs({ format: 'docx', filename: safeFilename(viewRef.current.title, 'docx'), assessment, error: '' });
        return;
      }
      const bytes = engine.current.exportFormat('docx', false, assessment);
      download(new Blob([bytes.slice().buffer], { type: DOCX_MEDIA_TYPE }), safeFilename(viewRef.current.title, 'docx'));
      setNotice('Word copy ready from current supported content. Export does not replace your local save.');
    } catch (failure) { setError(message(failure)); }
  }

  function downloadOriginal() {
    const source = viewRef.current?.originalSource;
    if (!engine.current || !source || !documentOpen || composing) return;
    try {
      const bytes = engine.current.exportOriginal();
      download(new Blob([bytes.slice().buffer], { type: formatMediaType(source.format) }), source.filename);
      setNotice('Exact original ready. Later edits are available through Save As and the local Writer save.');
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
      // An application shortcut must not reset an active format review, stack
      // another modal, or move focus behind the dialog into the editor.
      if (document.querySelector('dialog[open]')) {
        if (['s', 'o', 'f'].includes(event.key.toLowerCase())) event.preventDefault();
        return;
      }
      if (event.key.toLowerCase() === 's') {
        event.preventDefault();
        if (event.shiftKey) reviewSaveAs(); else void saveDocument();
      }
      if (event.key.toLowerCase() === 'o') { event.preventDefault(); void showLibrary(); }
      if (event.key.toLowerCase() === 'f' && documentOpen && view) {
        event.preventDefault();
        openFind();
      }
    }
    window.addEventListener('keydown', shortcut);
    return () => window.removeEventListener('keydown', shortcut);
  });

  const mobileFormattingPointer = useRef(false);
  function format(input: WriterCommand, refocus = true) {
    const next = command(input);
    // Focus synchronously; a deferred callback could steal focus from the next
    // keyboard control. The surface restores the new Rust selection on update.
    // A mobile toolbar tap can blur the hidden IME input before click fires.
    // Rust retains the selection; forcing focus from that click reopens the
    // software keyboard and can shift the viewport on iOS and Android.
    // Keep the existing desktop keyboard/mouse behavior.
    if (refocus && next && !mobileFormattingPointer.current) surface.current?.focus();
  }

  const unavailable = !view || !documentOpen || busy || composing;
  const formatState = view?.format;
  const importWarnings = view?.importWarnings;
  const importWarningCount = (importWarnings?.diagnostics.length ?? 0)
    + (importWarnings?.omittedDiagnostics ?? 0);
  const visibleError = error || (saveState.error
    ? `${message(saveState.error)} Autosave is paused. Your document is still open; retry Save or export a backup.` : '');
  const wordCountLabel = documentOpen && view
    ? `${view.wordCount.toLocaleString()} ${view.wordCount === 1 ? 'word' : 'words'}`
    : null;
  const saveLabel = !documentOpen ? 'No document open'
    : saveState.phase === 'error' ? 'Unsaved · autosave paused'
      : saveState.composing ? 'Composing · not saved'
        : saveState.phase === 'saving' ? 'Saving on this device…'
          : saveState.phase === 'pending' ? 'Unsaved changes · saving soon'
            : dirty ? 'Unsaved changes' : savedLocally ? 'Saved on this device' : 'Not saved yet';
  return <div className="writer-app" onKeyDownCapture={(event) => {
    if ((event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey
      && event.key.toLowerCase() === 'k' && !unavailable && !hyperlinkDialog && !paragraphDialog) {
      event.preventDefault();
      event.stopPropagation();
      openHyperlink();
    }
  }}>
    <header className="top-bar">
      <div className="identity">
        <img src="/graphe-icon.svg" className="writer-lantern" alt="" />
        <span className="identity__label">
          <span>Graphe</span>
          <span className="identity__office">Proairet<span className="office-brand__os">OS</span>&nbsp;Office</span>
        </span>
      </div>
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
          // Safari can blur the summary without focusing the tapped button.
          // Closing on that ambiguous null target removes it before click.
          // Outside pointerdown and known outside keyboard focus still dismiss.
          if (event.relatedTarget instanceof Node && !event.currentTarget.contains(event.relatedTarget)) {
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
            <button disabled={unavailable} onClick={() => documentAction(() => reviewSaveAs())}>Save As…</button>
            <button disabled={unavailable} onClick={() => documentAction(exportFile)}>Export</button>
            <button disabled={unavailable} onClick={() => documentAction(exportWordFile)}>Export Word</button>
            {view?.originalSource && <button disabled={unavailable} onClick={() => documentAction(downloadOriginal)}>Download original</button>}
            <button disabled={unavailable} onClick={() => documentAction(togglePagePreview)} aria-pressed={pagePreview}>Page preview</button>
            <button disabled={unavailable} onClick={() => documentAction(closeDocument)}>Close</button>
            <button type="button" onClick={() => { documentMenu.current?.removeAttribute("open"); setAboutOpen(true); }}>About Graphe</button>
          </div>
        </details>
      </nav>
      <div className="office-brand" aria-label="ProairetOS Office"><span>Proairet</span><span className="office-brand__os">OS</span><span className="office-brand__suite">{'\u00a0'}Office</span></div>
    </header>
    <nav className="editor-tabs" aria-label="Editor sections">
      <div className={`editor-tab-options editor-tab-options--${activeTab}`}>
        <span className="editor-tab-indicator" aria-hidden="true" />
        {(['write', 'insert', 'layout', 'review'] as const).map((tab) => <button key={tab}
          aria-pressed={activeTab === tab} aria-controls="writer-formatting" onClick={() => setActiveTab(tab)}>
          {tab.charAt(0).toUpperCase() + tab.slice(1)}</button>)}
      </div>
      <span className={`save-state${dirty ? ' is-unsaved' : ''}`} aria-live="polite">
        {saveLabel}
      </span>
    </nav>
    <div id="writer-formatting" ref={ribbonRef} className={`formatting-bar${moreFormatting ? ' show-more-formatting' : ''}`}
      onPointerDownCapture={(event) => { mobileFormattingPointer.current = event.pointerType === 'touch' || event.pointerType === 'pen'; }}
      onKeyDownCapture={() => { mobileFormattingPointer.current = false; }}
      aria-label={`${activeTab.charAt(0).toUpperCase() + activeTab.slice(1)} controls`}>
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
            {fontOptions(formatState?.fontFamily)}
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
        <fieldset disabled={unavailable} className="format-group list-controls advanced-formatting">
          <legend className="visually-hidden">Lists</legend>
          {(['bullet', 'numbered'] as const).map((list) => <button key={list}
            className="format-button" aria-label={list === 'bullet' ? 'Bulleted list' : 'Numbered list'}
            title={list === 'bullet' ? 'Bulleted list' : 'Numbered list'}
            aria-pressed={formatState?.listMixed ? 'mixed' : formatState?.list === list}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => format({ type: 'set_list', list: formatState?.list === list && !formatState?.listMixed ? null : list })}>
            <WriterIcon name={list === 'bullet' ? 'list-bullet' : 'list-numbered'} />
          </button>)}
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
      {activeTab === 'write' && <button className="more-formatting"
        aria-label={moreFormatting ? 'Hide text formatting' : 'Show text formatting'}
        title={moreFormatting ? 'Hide font and alignment controls' : 'Show font and alignment controls'}
        aria-expanded={moreFormatting}
        onClick={() => setMoreFormatting((value) => !value)}><WriterIcon name="text-format" /></button>}
      {activeTab === 'insert' && <div className="format-group insert-controls" role="group" aria-label="Insert text structure">
        <button disabled={unavailable} onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'insert_paragraph' })}>Paragraph <kbd>Enter</kbd></button>
        <button disabled={unavailable} onMouseDown={(event) => event.preventDefault()} onClick={() => format({ type: 'insert_line_break' })}>Line break <kbd>Shift Enter</kbd></button>
        <button disabled={unavailable} onMouseDown={(event) => event.preventDefault()}
          onClick={openHyperlink}><WriterIcon name="link" /> Link <kbd>Ctrl K</kbd></button>
      </div>}
      {activeTab === 'layout' && <div className="format-group paragraph-metrics-controls">
        <button disabled={unavailable} onMouseDown={(event) => event.preventDefault()}
          onClick={openParagraphDialog}>Paragraph spacing &amp; indents…</button>
        <span className="formatting-note">Page sizes and margins follow the document.</span>
      </div>}
      {activeTab === 'review' && <div className="format-group review-controls" role="group" aria-label="Review document">
        <button disabled={unavailable} onClick={openFind}><WriterIcon name="search" />Find</button>
        <button disabled={unavailable} aria-pressed={pagePreview} onClick={togglePagePreview}>Page preview</button>
      </div>}
      <button className="inspector-toggle" aria-controls="writer-inspector" aria-expanded={inspectorOpen}
        onClick={() => setInspectorOpen((open) => !open)} title="Toggle formatting inspector">
        {inspectorOpen ? 'Hide details' : 'Details'}
      </button>
    </div>
    {findOpen && view && documentOpen && <div ref={findPanel} className="find-panel"><FindBar view={view} onFind={findText} onCommand={command}
      onClose={() => setFindOpen(false)} onFocusDocument={() => surface.current?.focus()}
      onRevealSelection={(next) => surface.current?.revealSelection?.(next)} /></div>}
    {visibleError && !unsavedDialog && library === null && <div className="error-banner" role="alert"><span>{visibleError}</span>
      {saveConflict && <button onClick={() => void saveCopy()} disabled={busy}>Save a separate copy</button>}
      {saveState.error && <button disabled={busy || composing} onClick={() => void saveDocument()}>Retry save</button>}
      {error && <button aria-label="Dismiss error" onClick={() => setError('')}>×</button>}</div>}
    {preparedExport && <aside className="prepared-export" aria-label="Prepared export">
      <p role="status"><strong>{preparedExport.filename}</strong> is ready. {preparedExport.automaticFailed
        ? 'Your browser could not start the automatic download. Use a file link.'
        : 'Check Downloads or Files. If no file appeared, use a file link.'}
        <span>This copy does not include later edits and does not replace your local save.</span></p>
      <a href={preparedExport.url} download={preparedExport.filename} target="_blank" rel="noopener">Download file</a>
      <a href={preparedExport.url} target="_blank" rel="noopener">Open copy</a>
      <button className="icon-button" aria-label="Dismiss prepared export" onClick={dismissPreparedExport}>×</button>
    </aside>}
    {documentOpen && importWarnings && importWarningCount > 0 && <aside className="import-notice" aria-label="Import formatting notice" key={view?.sessionId}>
      <p role="status"><strong>Formatting adjusted for editing.</strong> Some original formatting uses disclosed approximations. Your exact original is retained.</p>
      <details>
        <summary>Review {importWarningCount.toLocaleString()} formatting {importWarningCount === 1 ? 'notice' : 'notices'}</summary>
        <ul>{importWarnings.diagnostics.map((warning, index) => <li key={`${warning.kind}-${index}`}>
          <strong>{warning.part}: {warning.path}</strong><span>{warning.message}</span>
        </li>)}</ul>
        {importWarnings.omittedDiagnostics > 0 && <p>{importWarnings.omittedDiagnostics.toLocaleString()} further notices were omitted from this bounded report.</p>}
        <p>Keep the native Writer file or download the original for full source preservation. New exported copies use the current supported document formatting.</p>
      </details>
    </aside>}
    <div className="writer-work-area"><main className="workspace" aria-label="Document workspace">
      {startupError ? <div className="startup-message" role="alert"><h1>Graphe could not start</h1><p>{startupError}</p><button onClick={() => window.location.reload()}>Try again</button></div>
        : !view ? <p className="startup-message" role="status">Opening Writer…</p>
        : documentOpen ? <><div className="paged-editing-view" hidden={pagePreview}><PagedEditor ref={surface} view={view} onCommand={command}
          getCurrentView={currentEngineView}
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
          : <div className="closed-document"><img src="/writer-logo.svg" className="writer-lantern" alt="" /><h1>Your document is closed</h1><p>Create a document or open one saved on this device.</p><div><button className="primary-button" onClick={newDocument}>New document</button><button onClick={() => void showLibrary()}>Open document</button></div></div>}
    </main>
    {inspectorOpen && <aside id="writer-inspector" className="writer-inspector" aria-label="Formatting inspector"
      onPointerDownCapture={(event) => { mobileFormattingPointer.current = event.pointerType === 'touch' || event.pointerType === 'pen'; }}
      onKeyDownCapture={() => { mobileFormattingPointer.current = false; }}>
      <div className="writer-inspector-heading">
        <div><span className="writer-inspector-eyebrow">Writer</span><h2>Formatting</h2></div>
        <button aria-label="Close formatting inspector" onClick={() => {
          ribbonRef.current?.querySelector<HTMLButtonElement>('.inspector-toggle')?.focus({ preventScroll: true });
          setInspectorOpen(false);
        }}>×</button>
      </div>
      <section className="writer-inspector-section" aria-label="Text formatting">
        <h3>Text</h3>
        <label htmlFor="inspector-style">Paragraph style</label>
        <select id="inspector-style" disabled={unavailable}
          value={formatState?.styleMixed ? 'mixed' : JSON.stringify(formatState?.styleId ?? null)}
          onChange={(event) => format({ type: 'assign_style', styleId: JSON.parse(event.target.value) as string | null }, false)}>
          {formatState?.styleMixed && <option value="mixed" disabled>Mixed styles</option>}
          <option value="null">Paragraph</option>
          {view?.styles.map((style) => <option key={style.id} value={JSON.stringify(style.id)}>{style.name}</option>)}
        </select>
        <label htmlFor="inspector-font-family">Font family</label>
        <select id="inspector-font-family" disabled={unavailable}
          value={formatState?.fontFamily ?? ''}
          onChange={(event) => format({ type: 'format_character', fontFamily: event.target.value }, false)}>
          {fontOptions(formatState?.fontFamily)}
        </select>
        <div className="font-family-preview" aria-label="Typeface sample"
          style={{ fontFamily: `"${bundledFontPreview(formatState?.fontFamily)}", sans-serif` }}>
          <span>{bundledFontSample(formatState?.fontFamily)}</span>
          <small>{formatState?.fontFamily && !BUNDLED_FONTS.has(formatState.fontFamily)
            ? 'Closest bundled preview' : formatState?.fontFamily ?? 'Mixed fonts'}</small>
        </div>
        <label htmlFor="inspector-font-size">Font size</label>
        <select id="inspector-font-size" disabled={unavailable} value={formatState?.fontSizePt ?? ''}
          onChange={(event) => format({ type: 'format_character', fontSizePt: Number(event.target.value) }, false)}>
          <option value="" disabled>Mixed</option>
          {[...new Set([8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72, ...(formatState?.fontSizePt ? [formatState.fontSizePt] : [])])].sort((a, b) => a - b).map((size) => <option key={size} value={size}>{size} pt</option>)}
        </select>
        <div className="writer-inspector-buttons" role="group" aria-label="Text emphasis">
          {(['bold', 'italic', 'underline'] as const).map((property) => <button key={property}
            disabled={unavailable} aria-label={property} aria-pressed={formatState?.[property] === null ? 'mixed' : !!formatState?.[property]}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => format({ type: 'format_character', [property]: !formatState?.[property] })}>
            {property === 'bold' ? 'B' : property === 'italic' ? 'I' : 'U'}
          </button>)}
        </div>
      </section>
      <section className="writer-inspector-section" aria-label="Paragraph formatting">
        <h3>Paragraph</h3>
        <div className="writer-inspector-buttons" role="group" aria-label="Alignment">
          {(['left', 'center', 'right', 'justify'] as const).map((alignment) => <button key={alignment}
            disabled={unavailable} aria-label={`Align ${alignment}`} aria-pressed={formatState?.alignment === alignment}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => format({ type: 'format_paragraph', alignment })}><AlignmentIcon alignment={alignment}/></button>)}
        </div>
        <button className="paragraph-metrics-open" disabled={unavailable}
          onClick={openParagraphDialog}>Spacing and indentation…</button>
      </section>
      <section className="writer-inspector-section">
        <h3>Page</h3>
        <p>Page size and margins follow the document. Use Page preview to inspect the layout.</p>
        <button disabled={unavailable} aria-pressed={pagePreview} onClick={togglePagePreview}>Page preview</button>
      </section>
    </aside>}
    </div>
    <footer className="status-bar">
      <span className="page-status" aria-live="polite">{!documentOpen ? 'No document open'
        : pageInfo.status === 'unavailable' ? 'Pages unavailable'
          : pageInfo.status === 'loading' ? 'Preparing pages…'
            : pageInfo.pageCount === 0 ? 'No pages'
              : `Page ${pageInfo.currentPage ?? '—'} of ${pageInfo.pageCount ?? '—'}`}</span>
      <span className="word-count">{wordCountLabel ?? 'Writer'}</span>
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
    <input ref={fileInput} type="file" accept={`.writer,.docx,.odt,.rtf,.txt,application/json,${DOCX_MEDIA_TYPE},application/vnd.oasis.opendocument.text,application/rtf,text/rtf,text/plain`} className="visually-hidden" tabIndex={-1} aria-label="Import Writer file"
      onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ''; if (file) importFile(file); }} />
    {library !== null && <Dialog title="Open document" onClose={() => setLibrary(null)} busy={busy}>
      <p className="dialog-description">Open a document saved in this browser, or choose a Writer, DOCX, ODT, RTF or TXT file. TXT supports UTF-8 or BOM-marked UTF-16.</p>
      {(visibleError || libraryProblems.length > 0) && <div className="dialog-error" role="alert">
        {visibleError && <p>{visibleError}</p>}
        {libraryProblems.length > 0 && <><p>{libraryProblems.length} saved {libraryProblems.length === 1 ? 'document could' : 'documents could'} not be listed.
          Their data remains on this device. Other documents can still be opened.</p>
          {libraryProblems.map((problem, index) => <p key={index}>{problem.message}</p>)}</>}
      </div>}
      <div className="document-library">{library.length === 0 ? <p>{libraryProblems.length ? 'No readable saved documents.' : 'No saved documents yet.'}</p> : library.map((entry) => <button className="library-entry" key={entry.id} disabled={busy}
        onClick={() => openSaved(entry.id)}><span>{entry.title || 'Untitled document'}</span><time dateTime={new Date(entry.updatedAt).toISOString()}>{new Date(entry.updatedAt).toLocaleString()}</time></button>)}</div>
      <p className="storage-note">Browser storage can be cleared or evicted. Keep exported .writer files as backups.</p>
      <div className="dialog-actions"><button disabled={busy} onClick={() => fileInput.current?.click()}>Open file…</button><button onClick={() => setLibrary(null)} disabled={busy}>Cancel</button></div>
    </Dialog>}
    {paragraphDialog && <Dialog title="Paragraph" className="save-copy-dialog paragraph-metrics-dialog"
      onClose={() => setParagraphDialog(null)} busy={busy}>
      <form className="paragraph-metrics-form" onSubmit={(event) => {
        event.preventDefault(); submitParagraphMetrics();
      }}>
        <fieldset>
          <legend>Spacing</legend>
          <div className="paragraph-metrics-grid">
            {PARAGRAPH_METRICS.slice(0, 2).map((field) => <label key={field.key}>
              <span>{field.label} ({field.unit})</span>
              <input type="number" inputMode="decimal" step="any" placeholder="Mixed" value={paragraphDialog.values[field.key]}
                onChange={(event) => setParagraphDialog({
                  ...paragraphDialog, error: '', values: { ...paragraphDialog.values,
                    [field.key]: event.target.value },
                })} />
            </label>)}
            <label><span>Line spacing</span>
              <select aria-label="Line spacing" value={paragraphDialog.values.lineSpacingMultiple}
                onChange={(event) => setParagraphDialog({ ...paragraphDialog, error: '',
                  values: { ...paragraphDialog.values, lineSpacingMultiple: event.target.value },
                })}>
                <option value="">Keep current / mixed</option>
                {[...new Set(['1', '1.15', '1.5', '2', '2.5', '3',
                  paragraphDialog.values.lineSpacingMultiple].filter(Boolean))].map((level) =>
                    <option key={level} value={level}>{level}×</option>)}
              </select>
            </label>
          </div>
        </fieldset>
        <fieldset>
          <legend>Indentation</legend>
          <div className="paragraph-metrics-grid">
            {PARAGRAPH_METRICS.slice(2).map((field) => <label key={field.key}>
              <span>{field.label} ({field.unit})</span>
              <input type="number" inputMode="decimal" step="any" placeholder="Mixed" value={paragraphDialog.values[field.key]}
                onChange={(event) => setParagraphDialog({
                  ...paragraphDialog, error: '', values: { ...paragraphDialog.values,
                    [field.key]: event.target.value },
                })} />
            </label>)}
          </div>
        </fieldset>
        <p className="paragraph-metrics-hint">
          A negative first-line value creates a hanging indent. Blank fields keep the
          current value for each paragraph.
        </p>
        {paragraphDialog.error && <p className="dialog-error" role="alert">
          {paragraphDialog.error}
        </p>}
        <div className="dialog-actions">
          <button type="button" onClick={() => setParagraphDialog(null)}>Cancel</button>
          <button type="submit" className="primary-button">Apply</button>
        </div>
      </form>
    </Dialog>}
    {hyperlinkDialog && <Dialog title={hyperlinkDialog.existing ? 'Edit hyperlink' : 'Insert hyperlink'}
      onClose={() => { setHyperlinkDialog(null); setError(''); }} className="save-copy-dialog hyperlink-dialog">
      <form className="hyperlink-fields" onSubmit={(event) => { event.preventDefault(); submitHyperlink(); }}>
        <label htmlFor="hyperlink-label">Text to display</label>
        <input id="hyperlink-label" value={hyperlinkDialog.label} maxLength={2048}
          placeholder={hyperlinkDialog.existing ? 'Existing linked text' : 'Link text'}
          onChange={(event) => setHyperlinkDialog({ ...hyperlinkDialog, label: event.target.value })} />
        <label htmlFor="hyperlink-address">Web address</label>
        <input id="hyperlink-address" inputMode="url" autoComplete="url" autoFocus
          value={hyperlinkDialog.url} maxLength={2048}
          onChange={(event) => setHyperlinkDialog({ ...hyperlinkDialog, url: event.target.value })} />
        <p className="dialog-description">HTTP and HTTPS links only. Selecting text preserves its formatting.</p>
        {error && <p role="alert" className="dialog-error">{error}</p>}
        <div className="dialog-actions">
          {hyperlinkDialog.existing && <>
            <button type="button" onClick={() => {
              const candidate = safeHyperlink(hyperlinkDialog.url) ? hyperlinkDialog.url : null;
              if (candidate) window.open(candidate, '_blank', 'noopener,noreferrer');
            }}>Open link</button>
            <button type="button" onClick={() => submitHyperlink(true)}>Remove link</button>
          </>}
          <button type="button" onClick={() => { setHyperlinkDialog(null); setError(''); }}>Cancel</button>
          <button type="submit" className="primary-button">Apply link</button>
        </div>
      </form>
    </Dialog>}
    {saveAs && <Dialog title="Save As" onClose={closeSaveAs} busy={busy} className="save-copy-dialog">
      <form onSubmit={(event) => { event.preventDefault(); downloadSaveAs(); }}>
      <div className="save-as-fields">
        <label htmlFor="save-as-filename">Filename</label>
        <input id="save-as-filename" value={saveAs.filename} maxLength={120} disabled={busy}
          onChange={(event) => setSaveAs({ ...saveAs, filename: event.target.value })} />
        <label htmlFor="save-as-format">File format</label>
        <select id="save-as-format" value={saveAs.format} disabled={busy}
          onChange={(event) => changeSaveAsFormat(event.target.value as DocumentFormat)}>
          {FILE_FORMATS.map((item) => <option key={item.format} value={item.format}>{item.label}</option>)}
        </select>
      </div>
      {!!saveAs.assessment?.warnings.length && <p className="save-copy-warning" id="save-copy-warning" role="note" aria-label="Format warning">
        {saveAs.assessment.warnings.map((warning) => warning.message).join(' ')}
      </p>}
      {saveAs.format === 'writer' && view?.originalSource && <p className="save-copy-hint" role="note" aria-label="Original included">
        Includes the imported original, which may contain removed text.
      </p>}
      {saveAs.error && <div className="dialog-error" role="alert"><p>{saveAs.error}</p>
        <button type="button" disabled={busy} onClick={() => reviewSaveAs(saveAs.format, saveAs.filename)}>Review current document</button></div>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={closeSaveAs}>Cancel</button>
        <button type="submit" className="primary-button" disabled={busy || !saveAs.assessment}
          aria-describedby={saveAs.assessment?.warnings.length ? 'save-copy-warning' : undefined}>Save copy</button></div>
      </form>
    </Dialog>}
    {aboutOpen && <Dialog title="About Graphe" onClose={() => setAboutOpen(false)} className="about-graphe-dialog">
      <div className="about-graphe-intro">
        <strong>Graphe</strong>
        <span>Version 0.1.0</span>
        <p>A free word processor from Proairetos.</p>
      </div>
      <section className="about-graphe-section" aria-label="Copyright and ownership">
        <h3>Copyright and ownership</h3>
        <p>© 2026 David Boley. All rights reserved.</p>
        <p>Graphe is proprietary software distributed under the Proairetos brand. It is free to use for personal and commercial work. Your documents and original content remain yours.</p>
      </section>
      <section className="about-graphe-section" aria-label="Licenses and notices">
        <h3>Licenses and notices</h3>
        <a href={grapheLicenseUrl} target="_blank" rel="noopener noreferrer">Graphe software license</a>
        <a href={fontLicensesUrl} target="_blank" rel="noopener noreferrer">Bundled font licenses</a>
        <a href={formatLicensesUrl} target="_blank" rel="noopener noreferrer">Third-party format library notices</a>
        <p>Third-party components retain their respective licenses.</p>
      </section>
      <div className="dialog-actions"><button className="primary-button" onClick={() => setAboutOpen(false)}>Done</button></div>
    </Dialog>}
    {exportDisclosure && <Dialog title="Export Writer file" onClose={() => setExportDisclosure(false)}>
      <p>This Writer file includes the preserved original {view?.originalSource ? formatName(view.originalSource.format) : 'document'}, {view?.originalSource?.filename}.</p>
      <p>The original may contain text or metadata removed from your edited document. Use Save As in another format when you need only current supported content.</p>
      {visibleError && <div className="dialog-error" role="alert">{visibleError}</div>}
      <div className="dialog-actions"><button onClick={() => setExportDisclosure(false)}>Cancel</button>
        <button className="primary-button" onClick={exportWriterFile}>Download Writer file</button></div>
    </Dialog>}
    {preservedImport && <Dialog title={`${formatName(preservedImport.format)} preserved read-only`} onClose={() => setPreservedImport(null)}>
      <p>{preservedImport.message}</p>
      <p>Your current document is unchanged. The original bytes remain available here for download. This read-only source has not been saved in the local document library.</p>
      <ul className="compatibility-issues">{preservedImport.report?.diagnostics.map((issue, index) =>
        <li key={index}><strong>{issue.part}: {issue.path}</strong><span>{issue.message}</span></li>)}</ul>
      {!!preservedImport.report?.omittedDiagnostics && <p>{preservedImport.report.omittedDiagnostics} additional issues were omitted from this bounded report.</p>}
      <div className="dialog-actions"><button onClick={() => download(preservedImport.file, preservedImport.file.name)}>Download original {formatName(preservedImport.format)}</button>
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
