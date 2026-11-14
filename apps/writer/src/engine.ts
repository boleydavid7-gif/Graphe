import initialize, { WriterEngine } from './wasm/office_wasm.js';
import wasmUrl from './wasm/office_wasm_bg.wasm?url';
import type { CompatibilityReport, EngineFailure, EngineResponse, WriterCommand, WriterView } from './types';

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
}
