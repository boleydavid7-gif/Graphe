import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import {
  MAX_STORED_DOCUMENT_BYTES, StorageError, WriterStorage,
  type DocumentBytes,
} from './storage';

const name = 'writer-storage-test';
const document = (id = 'document-1', text = 'writer bytes'): DocumentBytes => ({
  id, title: 'A warm lamp — 文書', bytes: new TextEncoder().encode(text),
});

let factory: IDBFactory;
let opened: WriterStorage[];

function storage(): WriterStorage {
  const result = new WriterStorage(name);
  opened.push(result);
  return result;
}

function rawOpen(version = 1): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = factory.open(name, version);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains('documents')) {
        request.result.createObjectStore('documents', { keyPath: 'id' });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function rawPut(value: unknown): Promise<void> {
  const database = await rawOpen();
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction('documents', 'readwrite');
    transaction.objectStore('documents').put(value);
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error);
  });
  database.close();
}

beforeEach(() => {
  factory = new IDBFactory();
  opened = [];
  vi.stubGlobal('indexedDB', factory);
});

afterEach(() => {
  for (const instance of opened) instance.close();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('WriterStorage', () => {
  it('round trips exact bytes, Unicode metadata, and opaque identifiers', async () => {
    const library = storage();
    const original = document('id["quotes"]:文書', '{"text":"á 👩🏽‍💻 العربية"}');
    const saved = await library.save(original, null);
    expect(saved.bytes).toEqual(original.bytes);
    expect(saved.id).toBe(original.id);
    expect(saved.title).toBe(original.title);
    expect(saved.etag).toBeTruthy();
    expect((await library.load(original.id))).toEqual(saved);
    expect(await library.list()).toEqual([{
      id: original.id, title: original.title, etag: saved.etag, updatedAt: saved.updatedAt,
    }]);
  });

  it('captures input bytes before asynchronous open and returns independent copies', async () => {
    const library = storage();
    const original = document();
    const expected = original.bytes.slice();
    const pending = library.save(original, null);
    original.bytes.fill(0);
    const saved = await pending;
    expect(saved.bytes).toEqual(expected);
    saved.bytes.fill(1);
    const loaded = await library.load(original.id);
    expect(loaded.bytes).toEqual(expected);
    loaded.bytes.fill(2);
    expect((await library.load(original.id)).bytes).toEqual(expected);
  });

  it('requires the current token to replace a record and changes it on every save', async () => {
    const library = storage();
    const first = await library.save(document(), null);
    await expect(library.save(document(undefined, 'imported copy'), null)).rejects.toMatchObject({ code: 'conflict' });
    const second = await library.save(document(undefined, 'revised bytes'), first.etag);
    expect(second.etag).not.toBe(first.etag);
    await expect(library.save(document(undefined, 'stale bytes'), first.etag)).rejects.toMatchObject({ code: 'conflict' });
    expect(await library.load(first.id)).toEqual(second);
  });

  it('serializes competing creates and updates across separate connections', async () => {
    const left = storage();
    const right = storage();
    const creates = await Promise.allSettled([
      left.save(document(undefined, 'left'), null),
      right.save(document(undefined, 'right'), null),
    ]);
    expect(creates.filter((result) => result.status === 'fulfilled')).toHaveLength(1);
    expect(creates.filter((result) => result.status === 'rejected')).toHaveLength(1);
    const initial = await left.load('document-1');
    const updates = await Promise.allSettled([
      left.save(document(undefined, 'left update'), initial.etag),
      right.save(document(undefined, 'right update'), initial.etag),
    ]);
    expect(updates.filter((result) => result.status === 'fulfilled')).toHaveLength(1);
    const rejected = updates.find((result) => result.status === 'rejected');
    expect(rejected?.status === 'rejected' ? rejected.reason : undefined).toMatchObject({ code: 'conflict' });
    const winner = updates.find((result) => result.status === 'fulfilled');
    expect(await right.load(initial.id)).toEqual(winner?.status === 'fulfilled' ? winner.value : undefined);
  });

  it('does not resolve at put success if the transaction subsequently aborts', async () => {
    const library = storage();
    const initial = await library.save(document(), null);
    const nativePut = IDBObjectStore.prototype.put;
    const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (this: IDBObjectStore, value: unknown, key?: IDBValidKey) {
      const request = key === undefined ? nativePut.call(this, value) : nativePut.call(this, value, key);
      request.addEventListener('success', () => this.transaction.abort());
      return request;
    });
    await expect(library.save(document(undefined, 'must roll back'), initial.etag)).rejects.toMatchObject({ code: 'io' });
    spy.mockRestore();
    expect(await library.load(initial.id)).toEqual(initial);
  });

  it('maps quota failures, aborts the save, and preserves the prior record', async () => {
    const library = storage();
    const initial = await library.save(document(), null);
    const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(() => {
      throw new DOMException('test quota', 'QuotaExceededError');
    });
    await expect(library.save(document(undefined, 'new'), initial.etag)).rejects.toMatchObject({ code: 'quota' });
    spy.mockRestore();
    expect(await library.load(initial.id)).toEqual(initial);
  });

  it('rejects missing records without creating or repairing them', async () => {
    const library = storage();
    await expect(library.load('absent')).rejects.toMatchObject({ code: 'not_found' });
    expect(await library.list()).toEqual([]);
    await expect(library.save(document(), 'missing-token')).rejects.toMatchObject({ code: 'conflict' });
    expect(await library.list()).toEqual([]);
  });

  it('reports corrupted record metadata without silently overwriting it', async () => {
    await rawPut({ id: 'broken', title: 'Broken', bytes: 'not byte data', etag: 'old', updatedAt: 1 });
    const library = storage();
    await expect(library.load('broken')).rejects.toMatchObject({ code: 'corrupt' });
    await expect(library.list()).rejects.toMatchObject({ code: 'corrupt' });
    await expect(library.save(document('broken'), 'old')).rejects.toMatchObject({ code: 'corrupt' });
  });

  it('rejects an out-of-range timestamp before UI date formatting and preserves the record', async () => {
    const original = {
      ...document('invalid-date'), etag: 'old-token', updatedAt: 8_640_000_000_000_001,
    };
    await rawPut(original);
    const library = storage();
    await expect(library.load(original.id)).rejects.toMatchObject({ code: 'corrupt' });
    await expect(library.list()).rejects.toMatchObject({ code: 'corrupt' });
    await expect(library.save(document(original.id, 'replacement'), original.etag)).rejects.toMatchObject({ code: 'corrupt' });
    const database = await rawOpen();
    const unchanged = await new Promise<unknown>((resolve, reject) => {
      const transaction = database.transaction('documents', 'readonly');
      const request = transaction.objectStore('documents').get(original.id);
      transaction.oncomplete = () => resolve(request.result);
      transaction.onabort = () => reject(transaction.error);
    });
    database.close();
    expect(unchanged).toEqual(original);
  });

  it('accepts the inclusive Date timestamp boundary', async () => {
    await rawPut({ ...document('last-date'), etag: 'token', updatedAt: 8_640_000_000_000_000 });
    const record = await storage().load('last-date');
    expect(new Date(record.updatedAt).toISOString()).toBe('+275760-09-13T00:00:00.000Z');
  });

  it('sorts summaries by newest save without exposing bytes', async () => {
    const library = storage();
    const clock = vi.spyOn(Date, 'now');
    clock.mockReturnValue(10);
    await library.save(document('old'), null);
    clock.mockReturnValue(20);
    await library.save(document('new'), null);
    const listed = await library.list();
    expect(listed.map((entry) => entry.id)).toEqual(['new', 'old']);
    expect(listed.every((entry) => !('bytes' in entry))).toBe(true);
  });

  it('enforces application size and token bounds before opening a transaction', async () => {
    const library = storage();
    await expect(library.save({ ...document(), bytes: new Uint8Array(0) }, null)).rejects.toMatchObject({ code: 'invalid' });
    await expect(library.save({ ...document(), bytes: new Uint8Array(MAX_STORED_DOCUMENT_BYTES + 1) }, null)).rejects.toMatchObject({ code: 'invalid' });
    await expect(library.save(document(), '')).rejects.toMatchObject({ code: 'invalid' });
    await expect(library.load('')).rejects.toMatchObject({ code: 'invalid' });
    expect(await library.list()).toEqual([]);
  });

  it('reports unavailable and denied browser storage clearly', async () => {
    vi.stubGlobal('indexedDB', undefined);
    await expect(storage().list()).rejects.toMatchObject({ code: 'unavailable' });
    vi.stubGlobal('indexedDB', factory);
    vi.spyOn(factory, 'open').mockImplementation(() => { throw new DOMException('denied', 'SecurityError'); });
    await expect(storage().list()).rejects.toMatchObject({ code: 'unavailable' });
  });

  it('reports blocked startup and closes a connection that opens after rejection', async () => {
    const database = await rawOpen();
    const close = vi.spyOn(database, 'close');
    const request = { result: database } as IDBOpenDBRequest;
    vi.spyOn(factory, 'open').mockImplementation(() => {
      queueMicrotask(() => request.onblocked?.call(request, new Event('blocked') as IDBVersionChangeEvent));
      return request;
    });
    await expect(storage().list()).rejects.toMatchObject({ code: 'blocked' });
    request.onsuccess?.call(request, new Event('success'));
    expect(close).toHaveBeenCalledOnce();
  });

  it('requires secure UUID support before creating overwrite tokens', async () => {
    vi.stubGlobal('crypto', undefined);
    await expect(storage().save(document(), null)).rejects.toMatchObject({ code: 'unavailable' });
  });

  it('closes on a database version change rather than blocking an upgrade', async () => {
    const library = storage();
    await library.list();
    const upgraded = await rawOpen(2);
    upgraded.close();
    await expect(library.list()).rejects.toMatchObject({ code: 'closed' });
  });

  it('rejects operations after close, including an in-progress database open', async () => {
    const library = storage();
    const pending = library.list();
    library.close();
    await expect(pending).rejects.toMatchObject({ code: 'closed' });
    await expect(library.load('anything')).rejects.toMatchObject({ code: 'closed' });
    await expect(library.save(document(), null)).rejects.toMatchObject({ code: 'closed' });
  });

  it('exposes structured errors as ordinary readable errors', () => {
    const error = new StorageError('conflict', 'A new copy is needed.');
    expect(error).toBeInstanceOf(Error);
    expect(error.name).toBe('StorageError');
    expect(error.message).toBe('A new copy is needed.');
    expect(error.code).toBe('conflict');
  });
});
