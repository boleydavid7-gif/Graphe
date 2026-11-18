import initialize, {
  WriterEngine, finalize_font_catalog, font_catalog_manifest, font_catalog_status, register_font,
} from './wasm/office_wasm.js';
import wasmUrl from './wasm/office_wasm_bg.wasm?url';
import { FontLoadError, loadFontCatalog, type FontCatalogManifest, type FontLoadProgress,
  type FontLoadResult } from './font-loader';
import type {
  CompatibilityReport, DocumentFormat, EngineFailure, EngineResponse, ExportAssessment, FindSnapshot, LayoutAction,
  LayoutActionView, LayoutOptions, LayoutSelectionView, LayoutView,
  WriterCommand, WriterView,
} from './types';

let initialization: Promise<unknown> | undefined;
let fontInitialization: Promise<FontLoadResult> | undefined;
let fontProgress: FontLoadProgress | undefined;
const fontListeners = new Set<(progress: FontLoadProgress) => void>();
const previewFaces = new Map<string, FontFace>();

interface BrowserFontManifest extends Omit<FontCatalogManifest, 'fonts'> {
  fonts: readonly (FontCatalogManifest['fonts'][number] & {
    family: string; style: string; weightMin: number; weightMax: number;
  })[];
}

function reportFontProgress(progress: FontLoadProgress): void {
  fontProgress = progress;
  for (const listener of fontListeners) listener(progress);
}

async function loadPreviewFace(font: BrowserFontManifest['fonts'][number], bytes: Uint8Array): Promise<void> {
  if (previewFaces.has(font.id)) return;
  if (typeof FontFace === 'undefined' || !document.fonts) {
    throw new Error('This browser cannot load the verified font catalog. Try a current browser.');
  }
  // Use the same verified bytes for semantic input/previews, including offline
  // cache hits. CSS points at these assets too; no host font is substituted.
  const face = new FontFace(font.family, bytes.slice().buffer, {
    style: font.style, weight: font.weightMin === font.weightMax ? String(font.weightMin)
      : `${font.weightMin} ${font.weightMax}`,
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([face.load(), new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`Preparing font ${font.id} timed out. Please retry.`)), 30_000);
    })]);
    document.fonts.add(face);
    previewFaces.set(font.id, face);
  } finally { clearTimeout(timer); }
}

async function initializeFonts(): Promise<FontLoadResult> {
  const manifest = JSON.parse(font_catalog_manifest()) as BrowserFontManifest;
  const status = JSON.parse(font_catalog_status()) as { ready: boolean; missingFontIds: string[] };
  // CSS-connected faces cannot be removed through FontFaceSet.delete. Remove
  // their URL rules before using the verified binary faces, so offline previews
  // never request an alternate copy. Stylesheets are same-origin under our CSP.
  const families = new Set(manifest.fonts.map((font) => font.family));
  for (const sheet of document.styleSheets) {
    for (let index = sheet.cssRules.length - 1; index >= 0; index--) {
      const rule = sheet.cssRules[index];
      if (rule instanceof CSSFontFaceRule
          && families.has(rule.style.fontFamily.replaceAll('"', '').replaceAll("'", ''))) sheet.deleteRule(index);
    }
  }
  const missing = new Set(status.missingFontIds);
  const fonts = new Map(manifest.fonts.map((font) => [font.id, font]));
  return loadFontCatalog(manifest, {
    onProgress: reportFontProgress,
    registerFont: async (id, bytes) => {
      if (!status.ready && missing.has(id)) register_font(id, bytes);
      await loadPreviewFace(fonts.get(id)!, bytes);
    },
    finalizeRegistration: () => {
      if (!status.ready) finalize_font_catalog();
      if (!(JSON.parse(font_catalog_status()) as { ready: boolean }).ready) {
        throw new Error('The required verified font catalog is incomplete. Please retry.');
      }
    },
  });
}

/** A structured engine failure; failed commands preserve the current session. */
export class EngineError extends Error {
  readonly code: string;
  readonly compatibility: CompatibilityReport | undefined;

  constructor(error: EngineFailure) {
    super(error.message);
    this.name = 'EngineError';
    this.code = error.code;
    this.compatibility = error.compatibility;
  }
}

function failure(error: unknown): EngineError {
  if (error instanceof EngineError) return error;
  if (error instanceof FontLoadError) return new EngineError({ code: `font_${error.code}`, message: error.message });
  if (typeof error === 'string') {
    try {
      const decoded = JSON.parse(error) as EngineFailure;
      if (typeof decoded.code === 'string' && typeof decoded.message === 'string') {
        return new EngineError(decoded);
      }
    } catch { /* The original string remains the diagnostic. */ }
  }
  return new EngineError({ code: 'engine_failure', message: error instanceof Error ? error.message : String(error) });
}

/** Owning client; snapshots are presentation values, never an editable JS model. */
export class EngineClient {
  private readonly handle: WriterEngine;
  private current: WriterView;
  private disposed = false;
  private readonly imageUrls = new Map<string, string>();
  private imageSession = '';
  readonly startupWarnings: readonly string[];

  private constructor(handle: WriterEngine, warnings: readonly string[]) {
    this.handle = handle;
    this.startupWarnings = warnings;
    this.current = JSON.parse(handle.snapshot()) as WriterView;
  }

  static async create(onFontProgress?: (progress: FontLoadProgress) => void): Promise<EngineClient> {
    if (onFontProgress) {
      fontListeners.add(onFontProgress);
      if (fontProgress) onFontProgress(fontProgress);
    }
    try {
      initialization ??= initialize({ module_or_path: wasmUrl }).catch((error: unknown) => {
        initialization = undefined;
        throw error;
      });
      await initialization;
      fontInitialization ??= initializeFonts().catch((error: unknown) => {
        fontInitialization = undefined;
        fontProgress = undefined;
        throw error;
      });
      const fonts = await fontInitialization;
      return new EngineClient(new WriterEngine(crypto.randomUUID()), fonts.warnings);
    } catch (error) {
      throw failure(error);
    } finally { if (onFontProgress) fontListeners.delete(onFontProgress); }
  }

  get view(): WriterView { return this.current; }

  /** Read-only Rust typography/page geometry for this exact document revision. */
  layout(options: LayoutOptions = {}): LayoutView {
    try {
      const result = JSON.parse(this.handle.layout_snapshot(
        JSON.stringify(options), this.current.revision,
      )) as LayoutView;
      if (result.sessionId !== this.current.sessionId
          || result.revision !== this.current.revision
          || result.documentId !== this.current.documentId
          || result.layout.documentId !== this.current.documentId) {
        throw new EngineError({
          code: 'stale_layout',
          message: 'The computed layout no longer identifies the active document.',
        });
      }
      return result;
    } catch (error) { throw failure(error); }
  }

  /** Applies Rust visual hit-testing/navigation without creating a document edit. */
  interact(action: LayoutAction, options: LayoutOptions = {},
    expectedOrigin?: Pick<WriterView, 'sessionId' | 'documentId' | 'revision'>): WriterView {
    try {
      if (expectedOrigin) this.assertOrigin(expectedOrigin);
      const result = JSON.parse(this.handle.layout_action(
        JSON.stringify(options), JSON.stringify(action), expectedOrigin?.revision ?? this.current.revision,
      )) as LayoutActionView;
      this.assertOrigin(result);
      this.assertOrigin(result.view);
      this.current = result.view;
      return this.current;
    } catch (error) { throw failure(error); }
  }

  /** Current selection/caret rectangles, measured and mapped entirely in Rust. */
  selectionGeometry(options: LayoutOptions = {}): LayoutSelectionView {
    try {
      const result = JSON.parse(this.handle.selection_geometry(
        JSON.stringify(options), this.current.revision,
      )) as LayoutSelectionView;
      this.assertOrigin(result);
      return result;
    } catch (error) { throw failure(error); }
  }

  /** Case-sensitive literal search returns Rust-validated document selections. */
  find(query: string): FindSnapshot {
    try {
      const result = JSON.parse(this.handle.find_text(query)) as FindSnapshot;
      this.assertOrigin(result);
      return result;
    } catch (error) { throw failure(error); }
  }

  command(command: WriterCommand): WriterView {
    const guarded = command.type === 'new_document' || command.type === 'fork_document'
      ? command
      : { ...command, expectedRevision: command.expectedRevision ?? this.current.revision };
    return this.accept(this.handle.dispatch(JSON.stringify(guarded)));
  }

  open(bytes: Uint8Array, expectedDocumentId?: string): WriterView {
    try { return this.accept(this.handle.open_document(bytes, expectedDocumentId)); }
    catch (error) { throw failure(error); }
  }

  serialize(): Uint8Array {
    try { return new Uint8Array(this.handle.export_document()); }
    catch (error) { throw failure(error); }
  }

  /** Rust validates, sizes and inserts raster bytes as one guarded transaction. */
  insertImage(bytes: Uint8Array, expectedOrigin: Pick<WriterView, 'sessionId' | 'documentId' | 'revision'>): WriterView {
    try {
      this.assertOrigin(expectedOrigin);
      return this.accept(this.handle.insert_image(bytes, expectedOrigin.revision));
    } catch (error) { throw failure(error); }
  }

  /** Presentation-only Blob URLs for immutable Rust assets, never document data. */
  presentationImages(expectedOrigin: Pick<WriterView, 'sessionId' | 'documentId' | 'revision'>): ReadonlyMap<string, string> {
    try {
      this.assertOrigin(expectedOrigin);
      if (this.imageSession !== this.current.sessionId) {
        this.releaseImageUrls();
        this.imageSession = this.current.sessionId;
      }
      const active = new Set((this.current.images ?? []).map((image) => image.id));
      for (const [id, url] of this.imageUrls) {
        if (!active.has(id)) { URL.revokeObjectURL(url); this.imageUrls.delete(id); }
      }
      for (const id of active) {
        if (!this.imageUrls.has(id)) {
          const bytes = new Uint8Array(this.handle.image_png(id, expectedOrigin.revision));
          const url = URL.createObjectURL(new Blob([bytes.slice().buffer], { type: 'image/png' }));
          this.imageUrls.set(id, url);
        }
      }
      return new Map(this.imageUrls);
    } catch (error) { throw failure(error); }
  }

  /** Creates a supported projection while Rust retains the exact original. */
  importDocx(bytes: Uint8Array, filename: string): WriterView {
    try { return this.accept(this.handle.import_docx(bytes, filename)); }
    catch (error) { throw failure(error); }
  }

  /** Regenerates current supported content, independently of the original. */
  exportDocx(): Uint8Array {
    try { return new Uint8Array(this.handle.export_docx()); }
    catch (error) { throw failure(error); }
  }

  /** Converts through the selected Rust adapter without deriving content in JS. */
  importFormat(format: DocumentFormat, bytes: Uint8Array, filename: string): WriterView {
    try { return this.accept(this.handle.import_format(bytes, format, filename)); }
    catch (error) { throw failure(error); }
  }

  /** Reports conversion losses for this exact immutable Rust document revision. */
  assessExport(format: DocumentFormat): ExportAssessment {
    try {
      const result = JSON.parse(this.handle.export_assessment(format, this.current.revision)) as ExportAssessment;
      this.assertOrigin(result);
      if (result.format !== format) throw new EngineError({ code: 'stale_export', message: 'The export format changed. Try Save As again.' });
      return result;
    } catch (error) { throw failure(error); }
  }

  /** Save copy confirms displayed meaningful losses; Rust checks the origin. */
  exportFormat(format: DocumentFormat, acknowledgeLoss = false,
    expectedOrigin?: Pick<WriterView, 'sessionId' | 'documentId' | 'revision'>): Uint8Array {
    try {
      if (expectedOrigin && (expectedOrigin.sessionId !== this.current.sessionId
          || expectedOrigin.documentId !== this.current.documentId
          || expectedOrigin.revision !== this.current.revision)) {
        throw new EngineError({ code: 'stale_revision',
          message: 'The document changed after Save As was prepared. Review its warnings again.' });
      }
      return new Uint8Array(this.handle.export_format(format, acknowledgeLoss,
        expectedOrigin?.revision ?? this.current.revision));
    } catch (error) { throw failure(error); }
  }

  /** Exact retained original bytes, including content removed by later edits. */
  exportOriginal(): Uint8Array {
    try { return new Uint8Array(this.handle.export_original()); }
    catch (error) { throw failure(error); }
  }

  /** Exact Rust-selected plain text, with one LF per paragraph boundary. */
  selectedText(): string {
    try { return this.handle.selected_text(); }
    catch (error) { throw failure(error); }
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.releaseImageUrls();
    this.handle.free();
  }

  private releaseImageUrls(): void {
    for (const url of this.imageUrls.values()) URL.revokeObjectURL(url);
    this.imageUrls.clear();
  }

  private accept(json: string): WriterView {
    const response = JSON.parse(json) as EngineResponse;
    if (!response.ok) throw new EngineError(response.error);
    this.current = response.view;
    return this.current;
  }

  private assertOrigin(result: { readonly sessionId: string; readonly documentId: string; readonly revision: string }): void {
    if (result.sessionId !== this.current.sessionId || result.documentId !== this.current.documentId
        || result.revision !== this.current.revision) {
      throw new EngineError({ code: 'stale_layout', message: 'The result no longer identifies the active document.' });
    }
  }
}
