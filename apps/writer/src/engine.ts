import initialize, { WriterEngine } from './wasm/office_wasm.js';
import wasmUrl from './wasm/office_wasm_bg.wasm?url';
import type {
  CompatibilityReport, DocumentFormat, EngineFailure, EngineResponse, ExportAssessment, FindSnapshot, LayoutAction,
  LayoutActionView, LayoutOptions, LayoutSelectionView, LayoutView,
  WriterCommand, WriterView,
} from './types';

let initialization: Promise<unknown> | undefined;

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

  private constructor(handle: WriterEngine) {
    this.handle = handle;
    this.current = JSON.parse(handle.snapshot()) as WriterView;
  }

  static async create(): Promise<EngineClient> {
    try {
      initialization ??= initialize({ module_or_path: wasmUrl });
      await initialization;
      return new EngineClient(new WriterEngine(crypto.randomUUID()));
    } catch (error) {
      initialization = undefined;
      throw failure(error);
    }
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
    this.handle.free();
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
