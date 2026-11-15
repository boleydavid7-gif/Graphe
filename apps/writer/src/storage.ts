/** Exact native document bytes in browser-local storage; Rust owns their format. */
export interface DocumentBytes {
  readonly id: string;
  readonly title: string;
  readonly bytes: Uint8Array;
}

export interface StoredDocument extends DocumentBytes {
  readonly etag: string;
  readonly updatedAt: number;
}

export type StoredDocumentSummary = Omit<StoredDocument, 'bytes'>;

/** A damaged record is reported without hiding or rewriting healthy documents. */
export interface LibraryProblem {
  readonly id?: string;
  readonly message: string;
}

export interface DocumentLibrary {
  readonly documents: readonly StoredDocumentSummary[];
  readonly problems: readonly LibraryProblem[];
}

export type StorageErrorCode =
  | 'unavailable' | 'blocked' | 'closed' | 'not_found' | 'conflict'
  | 'corrupt' | 'quota' | 'invalid' | 'io';

export class StorageError extends Error {
  readonly code: StorageErrorCode;

  constructor(code: StorageErrorCode, message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'StorageError';
    this.code = code;
  }
}

/** Application limit, separate from the native Writer format specification. */
export const MAX_STORED_DOCUMENT_BYTES = 5 * 1024 * 1024;
const STORE = 'documents';
// ECMAScript Date TimeClip range; metadata must be safe for ISO/UI formatting.
const MAX_DATE_TIMESTAMP = 8_640_000_000_000_000;

function failure(cause: unknown): StorageError {
  if (cause instanceof StorageError) return cause;
  const name = cause instanceof Error ? cause.name : '';
  if (name === 'QuotaExceededError') {
    return new StorageError('quota', 'Browser storage is full. Export your document to keep a copy.', { cause });
  }
  if (name === 'SecurityError' || name === 'NotAllowedError') {
    return new StorageError('unavailable', 'Browser-local storage is unavailable. Export your document to keep a copy.', { cause });
  }
  return new StorageError('io', 'The browser could not complete the storage operation. Your open document is unchanged.', { cause });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function decode(value: unknown): StoredDocument {
  if (!isRecord(value)
    || typeof value.id !== 'string' || value.id.length === 0
    || typeof value.title !== 'string'
    || typeof value.etag !== 'string' || value.etag.length === 0
    || typeof value.updatedAt !== 'number' || !Number.isFinite(value.updatedAt)
    || value.updatedAt < 0 || value.updatedAt > MAX_DATE_TIMESTAMP
    || !(value.bytes instanceof Uint8Array)
    || value.bytes.byteLength === 0 || value.bytes.byteLength > MAX_STORED_DOCUMENT_BYTES) {
    throw new StorageError('corrupt', 'This local document record is damaged. Open an exported .writer copy instead.');
  }
  return {
    id: value.id,
    title: value.title,
    bytes: value.bytes.slice(),
    etag: value.etag,
    updatedAt: value.updatedAt,
  };
}

function summary(record: StoredDocument): StoredDocumentSummary {
  return { id: record.id, title: record.title, etag: record.etag, updatedAt: record.updatedAt };
}

/**
 * Explicit saves with optimistic overwrite protection. A save resolves only on
 * transaction completion, never merely when a put request succeeds. Storage
 * completion is not a guarantee against browser eviction or power loss.
 */
export class WriterStorage {
  private readonly databaseName: string;
  private connection: Promise<IDBDatabase> | undefined;
  private closed = false;

  constructor(databaseName = 'writer-documents') {
    this.databaseName = databaseName;
  }

  private async database(): Promise<IDBDatabase> {
    if (this.closed) {
      throw new StorageError('closed', 'The local document library was closed. Reload Writer to reconnect.');
    }
    if (!this.connection) {
      const opening = this.open();
      this.connection = opening;
      // Opening may fail transiently. Keep automatic retry policy in the save
      // controller, but allow the next explicit operation to make a fresh try.
      void opening.catch(() => {
        if (this.connection === opening) this.connection = undefined;
      });
    }
    const database = await this.connection;
    if (this.closed) {
      throw new StorageError('closed', 'The local document library was closed. Reload Writer to reconnect.');
    }
    return database;
  }

  private open(): Promise<IDBDatabase> {
    const opening: Promise<IDBDatabase> = new Promise((resolve, reject) => {
      if (!globalThis.indexedDB) {
        reject(new StorageError('unavailable', 'This browser does not provide local document storage. You can still export .writer files.'));
        return;
      }
      let request: IDBOpenDBRequest;
      let abandoned = false;
      try {
        request = globalThis.indexedDB.open(this.databaseName, 1);
      } catch (cause) {
        reject(failure(cause));
        return;
      }
      request.onupgradeneeded = () => {
        if (!request.result.objectStoreNames.contains(STORE)) {
          request.result.createObjectStore(STORE, { keyPath: 'id' });
        }
      };
      request.onblocked = () => {
        abandoned = true;
        reject(new StorageError('blocked', 'Another Writer tab is blocking the local library. Close older tabs, then reload.'));
      };
      request.onerror = () => reject(failure(request.error));
      request.onsuccess = () => {
        const database = request.result;
        if (abandoned || this.closed) {
          database.close();
          if (this.closed) reject(new StorageError('closed', 'The local document library was closed.'));
          return;
        }
        database.onversionchange = () => {
          database.close();
          this.closed = true;
        };
        database.onclose = () => {
          // An abnormal platform close invalidates this connection, not the
          // live document or its etag. Explicit close/version changes remain
          // terminal; a later Save can reconnect after an unexpected close.
          if (this.connection === opening) this.connection = undefined;
        };
        resolve(database);
      };
    });
    return opening;
  }

  /** List metadata and explicit record warnings without rewriting native bytes. */
  async list(): Promise<DocumentLibrary> {
    const database = await this.database();
    return new Promise((resolve, reject) => {
      let transaction: IDBTransaction;
      let result: DocumentLibrary = { documents: [], problems: [] };
      let reason: StorageError | undefined;
      try {
        transaction = database.transaction(STORE, 'readonly');
        const request = transaction.objectStore(STORE).getAll();
        request.onsuccess = () => {
          try {
            const documents: StoredDocumentSummary[] = [];
            const problems: LibraryProblem[] = [];
            for (const value of request.result as unknown[]) {
              try {
                documents.push(summary(decode(value)));
              } catch (cause) {
                if (!(cause instanceof StorageError) || cause.code !== 'corrupt') throw cause;
                const id = isRecord(value) && typeof value.id === 'string' && value.id.length > 0 ? value.id : undefined;
                problems.push({ message: cause.message, ...(id ? { id } : {}) });
              }
            }
            documents.sort((left, right) => right.updatedAt - left.updatedAt || left.id.localeCompare(right.id));
            result = { documents, problems };
          } catch (cause) {
            reason = failure(cause);
            transaction.abort();
          }
        };
      } catch (cause) {
        reject(failure(cause));
        return;
      }
      transaction.oncomplete = () => resolve(result);
      transaction.onabort = () => reject(reason ?? failure(transaction.error));
      transaction.onerror = (event) => { reason ??= failure((event.target as IDBRequest).error ?? transaction.error); };
    });
  }

  async load(id: string): Promise<StoredDocument> {
    if (!id) throw new StorageError('invalid', 'Choose a document to open.');
    const database = await this.database();
    return new Promise((resolve, reject) => {
      let transaction: IDBTransaction;
      let result: StoredDocument | undefined;
      let reason: StorageError | undefined;
      try {
        transaction = database.transaction(STORE, 'readonly');
        const request = transaction.objectStore(STORE).get(id);
        request.onsuccess = () => {
          try {
            if (request.result === undefined) {
              throw new StorageError('not_found', 'This document is no longer in the local library.');
            }
            result = decode(request.result);
            if (result.id !== id) throw new StorageError('corrupt', 'The stored document identifier does not match its record.');
          } catch (cause) {
            reason = failure(cause);
            transaction.abort();
          }
        };
      } catch (cause) {
        reject(failure(cause));
        return;
      }
      transaction.oncomplete = () => {
        if (result) resolve(result);
        else reject(new StorageError('io', 'The browser returned no document.'));
      };
      transaction.onabort = () => reject(reason ?? failure(transaction.error));
      transaction.onerror = (event) => { reason ??= failure((event.target as IDBRequest).error ?? transaction.error); };
    });
  }

  /**
   * expectedEtag=null creates a new record only. Existing records require the
   * token returned by load/save, so importing the same ID cannot overwrite it.
   * The comparison and put share one read/write transaction across browser tabs.
   */
  async save(document: DocumentBytes, expectedEtag: string | null): Promise<StoredDocument> {
    if (typeof document.id !== 'string' || document.id.length === 0 || typeof document.title !== 'string'
      || !(document.bytes instanceof Uint8Array) || document.bytes.byteLength === 0
      || document.bytes.byteLength > MAX_STORED_DOCUMENT_BYTES
      || (expectedEtag !== null && (typeof expectedEtag !== 'string' || expectedEtag.length === 0))) {
      throw new StorageError('invalid', 'This document cannot be saved locally. Writer documents must be at most 5 MiB.');
    }
    if (!globalThis.crypto?.randomUUID) {
      throw new StorageError('unavailable', 'Safe local saving requires a secure browser context. Export a .writer copy instead.');
    }
    // Capture before awaiting database startup; caller mutation cannot change a save.
    let record: StoredDocument;
    try {
      record = {
        id: document.id,
        title: document.title,
        bytes: document.bytes.slice(),
        etag: globalThis.crypto.randomUUID(),
        updatedAt: Date.now(),
      };
    } catch (cause) {
      throw failure(cause);
    }
    const database = await this.database();
    return new Promise((resolve, reject) => {
      let transaction: IDBTransaction;
      let reason: StorageError | undefined;
      try {
        transaction = database.transaction(STORE, 'readwrite');
        const store = transaction.objectStore(STORE);
        const request = store.get(record.id);
        request.onsuccess = () => {
          try {
            const previous = request.result === undefined ? undefined : decode(request.result);
            if ((previous?.etag ?? null) !== expectedEtag) {
              throw new StorageError('conflict', 'Another saved version already exists. Reopen it, or save your current work as a separate copy.');
            }
            store.put(record);
          } catch (cause) {
            reason = failure(cause);
            transaction.abort();
          }
        };
      } catch (cause) {
        reject(failure(cause));
        return;
      }
      transaction.oncomplete = () => resolve({ ...record, bytes: record.bytes.slice() });
      transaction.onabort = () => reject(reason ?? failure(transaction.error));
      transaction.onerror = (event) => { reason ??= failure((event.target as IDBRequest).error ?? transaction.error); };
    });
  }

  /** Finish existing transactions and release this tab's connection. */
  close(): void {
    this.closed = true;
    void this.connection?.then((database) => database.close(), () => undefined);
  }
}
