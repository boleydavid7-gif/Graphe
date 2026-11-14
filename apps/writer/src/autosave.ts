import { StorageError, type StoredDocument, type WriterStorage } from './storage';

/** A live Rust revision. These values never describe an editable JS document. */
export interface SaveOrigin {
  readonly sessionId: string;
  readonly documentId: string;
  readonly revision: string;
}

/** Captured synchronously at the beginning of an actual storage attempt. */
export interface SaveCapture extends SaveOrigin {
  readonly title: string;
  readonly serialize: () => Uint8Array;
}

/** The token returned by the last committed save/load of this exact session. */
export interface SaveAssociation {
  readonly sessionId: string;
  readonly id: string;
  readonly etag: string;
}

export interface SaveState {
  readonly phase: 'idle' | 'pending' | 'saving' | 'saved' | 'error';
  /** Includes visible, uncommitted IME work while composition is active. */
  readonly dirty: boolean;
  readonly savedLocally: boolean;
  readonly composing: boolean;
  readonly error: Error | null;
}

export interface SaveCommit {
  readonly origin: SaveOrigin;
  readonly association: SaveAssociation;
  /** True only when no newer Rust revision or visible composition remains unsaved. */
  readonly current: boolean;
  readonly updatedAt: number;
}

export interface SaveControllerOptions {
  readonly storage: Pick<WriterStorage, 'save'>;
  readonly current: () => SaveCapture | null;
  readonly onState: (state: SaveState) => void;
  readonly onCommit?: (commit: SaveCommit) => void;
  readonly delayMs?: number;
}

function identityMatches(left: SaveOrigin | null, right: SaveOrigin | null): boolean {
  return left !== null && right !== null
    && left.sessionId === right.sessionId && left.documentId === right.documentId;
}

function origin(capture: SaveOrigin): SaveOrigin {
  return { sessionId: capture.sessionId, documentId: capture.documentId, revision: capture.revision };
}

function readableError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}

/**
 * One bounded save lane shared by explicit Save and debounced autosave.
 *
 * WriterStorage resolves only after IndexedDB transaction completion. No state
 * here claims crash durability, prevents browser eviction, or persists history.
 * Autosave waits for 750 ms of inactivity, with a 5 s maximum pending wait while
 * foreground timers run. Composition pauses both timers. A failed attempt pauses
 * automatic retries until explicit Save. A drain writes at most twice; any
 * remaining edits return to the ordinary debounce/deadline window.
 */
export class WriterSaveController {
  private readonly options: SaveControllerOptions;
  private readonly delayMs: number;
  private binding: SaveOrigin | null = null;
  private association: SaveAssociation | null = null;
  private observedRevision: string | null = null;
  private savedRevision: string | null = null;
  private dirty = false;
  private composing = false;
  private phase: SaveState['phase'] = 'idle';
  private error: Error | null = null;
  private epoch = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private deadline: ReturnType<typeof setTimeout> | undefined;
  private running: Promise<boolean> | undefined;
  private followUp = false;
  private disposed = false;

  constructor(options: SaveControllerOptions) {
    this.options = options;
    this.delayMs = options.delayMs ?? 750;
    if (!Number.isFinite(this.delayMs) || this.delayMs < 0) {
      throw new StorageError('invalid', 'The autosave delay must be a finite nonnegative number.');
    }
  }

  get state(): SaveState {
    return {
      phase: this.error ? 'error' : this.composing ? 'pending' : this.phase,
      dirty: this.dirty || this.composing,
      savedLocally: this.association !== null,
      composing: this.composing,
      error: this.error,
    };
  }

  /** Call after replacing the live view; a pristine unsaved document stays idle. */
  reset(association: SaveAssociation | null): void {
    if (this.disposed) return;
    const captured = this.options.current();
    if (association && (!captured || association.sessionId !== captured.sessionId
      || association.id !== captured.documentId || association.etag.length === 0)) {
      throw new StorageError('invalid', 'The local save token does not identify the active document session.');
    }
    this.epoch += 1;
    this.cancelPending();
    this.binding = captured ? origin(captured) : null;
    this.association = association ? { ...association } : null;
    this.observedRevision = captured?.revision ?? null;
    this.savedRevision = association ? captured?.revision ?? null : null;
    this.dirty = false;
    this.composing = false;
    this.error = null;
    this.phase = association ? 'saved' : 'idle';
    this.emit();
  }

  /** Notify a Rust revision change; selection-only updates do not schedule writes. */
  changed(): void {
    if (this.disposed) return;
    try {
      const captured = this.options.current();
      if (!captured) return;
      const newBinding = !identityMatches(this.binding, captured);
      if (newBinding) this.reset(null);
      if (!newBinding && this.observedRevision === captured.revision) return;
      this.observedRevision = captured.revision;
      this.dirty = true;
      if (!this.error) {
        if (this.phase !== 'saving') this.phase = 'pending';
        this.schedule();
      }
      this.emit();
    } catch (cause) { this.fail(cause); }
  }

  /** IME work remains transient: do not serialize it or report a false Saved state. */
  setComposing(active: boolean): void {
    if (this.disposed || this.composing === active) return;
    this.composing = active;
    if (active) this.clearTimer();
    else this.schedule();
    this.emit();
  }

  /** Explicit Save/retry. True only when the active session's latest revision is saved. */
  flush(): Promise<boolean> { return this.request(true); }

  /** Best-effort lifecycle flush; a prior failure is never retried automatically. */
  flushPending(): Promise<boolean> { return this.request(false); }

  /** Cancels unsent work. An IndexedDB transaction already underway may complete. */
  cancelPending(): void {
    this.clearTimer();
    this.followUp = false;
  }

  /** Silence callbacks before disposing the engine or closing storage. */
  dispose(): void {
    this.disposed = true;
    this.epoch += 1;
    this.cancelPending();
  }

  private emit(): void {
    if (!this.disposed) this.options.onState(this.state);
  }

  private clearTimer(): void {
    clearTimeout(this.timer);
    clearTimeout(this.deadline);
    this.timer = undefined;
    this.deadline = undefined;
  }

  private schedule(): void {
    if (this.disposed || this.composing || this.error || !this.dirty) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.request(false);
    }, this.delayMs);
    // Active typing may keep resetting the short debounce. Do not postpone
    // every save indefinitely; this foreground timer shares the same write lane.
    this.deadline ??= setTimeout(() => {
      this.deadline = undefined;
      void this.request(false);
    }, 5_000);
  }

  private request(explicit: boolean): Promise<boolean> {
    if (this.disposed || this.composing || (!explicit && (this.error || !this.dirty))) {
      return Promise.resolve(false);
    }
    let captured: SaveCapture | null;
    try { captured = this.options.current(); }
    catch (cause) { this.fail(cause); return Promise.resolve(false); }
    if (!captured) return Promise.resolve(false);
    if (!identityMatches(this.binding, captured)) this.reset(null);
    const requestedEpoch = this.epoch;
    const requestedOrigin = origin(captured);
    this.clearTimer();
    if (explicit && this.error) {
      this.error = null;
      this.phase = 'pending';
      this.emit();
    }
    this.followUp = true;
    if (!this.running) {
      // Enter on a microtask so the shared lane exists before callbacks run.
      this.running = Promise.resolve().then(() => this.drain()).finally(() => {
        this.running = undefined;
        if (this.dirty && !this.timer) this.schedule();
      });
    }
    return this.running.then(() => {
      if (this.disposed || requestedEpoch !== this.epoch) return false;
      const current = this.options.current();
      return identityMatches(requestedOrigin, current) && !this.composing && !this.dirty
        && this.association !== null && this.savedRevision === current?.revision;
    });
  }

  private async drain(): Promise<boolean> {
    let saved = false;
    for (let attempt = 0; attempt < 2 && this.followUp; attempt += 1) {
      this.followUp = false;
      if (this.disposed || this.composing || this.error) break;
      saved = await this.saveCurrent();
    }
    return saved;
  }

  private async saveCurrent(): Promise<boolean> {
    let captured: SaveCapture | null;
    try { captured = this.options.current(); }
    catch (cause) { this.fail(cause); return false; }
    if (!captured || this.disposed || this.composing) return false;
    if (!identityMatches(this.binding, captured)) this.reset(null);
    if (this.association && !this.dirty && this.savedRevision === captured.revision) {
      this.phase = 'saved';
      this.emit();
      return true;
    }
    const capturedEpoch = this.epoch;
    const capturedOrigin = origin(captured);
    // Resolve the token when execution begins, after any preceding save succeeds.
    const expected = this.association?.etag ?? null;
    this.phase = 'saving';
    this.emit();
    let saved: StoredDocument;
    try {
      const bytes = captured.serialize().slice();
      saved = await this.options.storage.save({ id: captured.documentId, title: captured.title, bytes }, expected);
    } catch (cause) {
      if (this.isCurrent(capturedEpoch, capturedOrigin)) this.fail(cause);
      return false;
    }
    if (!this.isCurrent(capturedEpoch, capturedOrigin)) return false;
    const current = this.options.current();
    this.association = { sessionId: captured.sessionId, id: saved.id, etag: saved.etag };
    this.savedRevision = captured.revision;
    this.dirty = current?.revision !== captured.revision;
    this.error = null;
    this.phase = this.dirty ? 'pending' : 'saved';
    this.emit();
    if (this.isCurrent(capturedEpoch, capturedOrigin)) {
      this.options.onCommit?.({ origin: capturedOrigin, association: { ...this.association },
        current: !this.dirty && !this.composing, updatedAt: saved.updatedAt });
    }
    return !this.dirty && !this.composing;
  }

  private isCurrent(epoch: number, captured: SaveOrigin): boolean {
    return !this.disposed && epoch === this.epoch && identityMatches(captured, this.options.current());
  }

  private fail(cause: unknown): void {
    if (this.disposed) return;
    this.error = readableError(cause);
    this.dirty = true;
    this.phase = 'error';
    this.cancelPending();
    this.emit();
  }
}
