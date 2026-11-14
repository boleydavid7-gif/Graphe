import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { WriterSaveController, type SaveCapture, type SaveCommit, type SaveState } from './autosave';
import { StorageError, WriterStorage, type DocumentBytes, type StoredDocument } from './storage';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function stored(document: DocumentBytes, etag: string): StoredDocument {
  return { ...document, bytes: document.bytes.slice(), etag, updatedAt: 123 };
}

function controlledStorage() {
  const pending: { document: DocumentBytes; expected: string | null; done: ReturnType<typeof deferred<StoredDocument>> }[] = [];
  const save = vi.fn((document: DocumentBytes, expected: string | null) => {
    const done = deferred<StoredDocument>();
    pending.push({ document, expected, done });
    return done.promise;
  });
  return { save, pending, finish(index: number, etag: string) {
    const request = pending[index];
    if (!request) throw new Error('No pending save');
    request.done.resolve(stored(request.document, etag));
  } };
}

function harness(storage: Pick<WriterStorage, 'save'> = {
  save: vi.fn(async (value) => stored(value, 'first-token')),
}) {
  let live = { sessionId: 'session-a', documentId: 'document-a', revision: 'r1', title: 'Untitled',
    bytes: new TextEncoder().encode('native: initial') };
  const states: SaveState[] = [];
  const commits: SaveCommit[] = [];
  const serialize = vi.fn((bytes: Uint8Array) => bytes);
  const current = vi.fn((): SaveCapture | null => {
    const captured = live;
    return { ...captured, serialize: () => serialize(captured.bytes) };
  });
  const controller = new WriterSaveController({ storage, current,
    onState: (state) => states.push(state), onCommit: (commit) => commits.push(commit) });
  controllers.push(controller);
  controller.reset(null);
  return {
    controller, current, serialize, states, commits, storage,
    get live() { return live; },
    set(value: Partial<typeof live>, notify = true) {
      live = { ...live, ...value };
      if (notify) controller.changed();
    },
    edit(revision: string, text = revision) {
      live = { ...live, revision, bytes: new TextEncoder().encode(text) };
      controller.changed();
    },
  };
}

async function ticks() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

let controllers: WriterSaveController[];
let libraries: WriterStorage[];

beforeEach(() => {
  controllers = [];
  libraries = [];
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  vi.stubGlobal('indexedDB', new IDBFactory());
});

afterEach(() => {
  controllers.forEach((controller) => controller.dispose());
  libraries.forEach((library) => library.close());
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('WriterSaveController', () => {
  it('does not autosave pristine startup, but explicit Save can save a blank document', async () => {
    const h = harness();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(await h.controller.flushPending()).toBe(false);
    expect(h.storage.save).not.toHaveBeenCalled();
    expect(h.controller.state).toMatchObject({ phase: 'idle', dirty: false, savedLocally: false });
    expect(await h.controller.flush()).toBe(true);
    expect(h.storage.save).toHaveBeenCalledOnce();
    expect(h.controller.state).toMatchObject({ phase: 'saved', dirty: false, savedLocally: true });
  });

  it('debounces changes and serializes only the latest revision when saving begins', async () => {
    const h = harness();
    h.edit('r2', 'discarded debounce candidate');
    await vi.advanceTimersByTimeAsync(700);
    h.edit('r3', 'Cafe\u0301 👩🏽‍💻 العربية');
    await vi.advanceTimersByTimeAsync(749);
    expect(h.storage.save).not.toHaveBeenCalled();
    expect(h.serialize).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    await h.controller.flush();
    expect(h.storage.save).toHaveBeenCalledOnce();
    expect(h.serialize).toHaveBeenCalledOnce();
    expect(h.storage.save).toHaveBeenCalledWith({ id: 'document-a', title: 'Untitled',
      bytes: new TextEncoder().encode('Cafe\u0301 👩🏽‍💻 العربية') }, null);
  });

  it('does not schedule saves for repeated selection-only notifications', async () => {
    const h = harness();
    h.controller.changed();
    h.controller.changed();
    await vi.advanceTimersByTimeAsync(2_000);
    expect(h.storage.save).not.toHaveBeenCalled();
    expect(h.controller.state.dirty).toBe(false);
  });

  it('saves the latest pending revision within the maximum wait during continuous typing', async () => {
    const h = harness();
    h.edit('r2');
    for (let revision = 3; revision <= 11; revision += 1) {
      await vi.advanceTimersByTimeAsync(500);
      h.edit(`r${revision}`);
    }
    expect(h.storage.save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(500);
    expect(h.storage.save).toHaveBeenCalledOnce();
    expect(h.storage.save).toHaveBeenCalledWith(expect.objectContaining({ bytes: new TextEncoder().encode('r11') }), null);
    expect(h.controller.state.dirty).toBe(false);
  });

  it('composition and document replacement cancel the maximum-wait timer as well as debounce', async () => {
    const h = harness();
    h.edit('r2');
    await vi.advanceTimersByTimeAsync(500);
    h.controller.setComposing(true);
    await vi.advanceTimersByTimeAsync(6_000);
    expect(h.storage.save).not.toHaveBeenCalled();
    h.controller.setComposing(false);
    await vi.advanceTimersByTimeAsync(500);
    h.set({ sessionId: 'new-session', documentId: 'new-document', revision: 'new-r1' }, false);
    h.controller.reset(null);
    await vi.advanceTimersByTimeAsync(6_000);
    expect(h.storage.save).not.toHaveBeenCalled();
    expect(h.controller.state).toMatchObject({ phase: 'idle', dirty: false });
  });

  it('waits for transaction completion and owns immutable captured bytes', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2', 'exact bytes including original source');
    const original = h.live.bytes.slice();
    const saved = h.controller.flush();
    await ticks();
    expect(h.controller.state).toMatchObject({ phase: 'saving', dirty: true, savedLocally: false });
    expect(h.commits).toEqual([]);
    h.live.bytes.fill(0);
    expect(local.pending[0]?.document.bytes).toEqual(original);
    local.finish(0, 'etag-1');
    expect(await saved).toBe(true);
    expect(h.controller.state).toMatchObject({ phase: 'saved', dirty: false, savedLocally: true });
  });

  it('coalesces edits during a save and chains the completed token into the latest write', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2');
    const first = h.controller.flush();
    await ticks();
    for (let index = 3; index <= 8; index += 1) {
      h.edit(`r${index}`);
      await vi.advanceTimersByTimeAsync(750);
    }
    expect(local.save).toHaveBeenCalledTimes(1);
    local.finish(0, 'etag-1');
    await ticks();
    expect(local.save).toHaveBeenCalledTimes(2);
    expect(local.pending[1]).toMatchObject({ expected: 'etag-1', document: { bytes: new TextEncoder().encode('r8') } });
    expect(h.commits[0]).toMatchObject({ origin: { revision: 'r2' }, current: false });
    expect(h.controller.state.dirty).toBe(true);
    local.finish(1, 'etag-2');
    expect(await first).toBe(true);
    expect(h.serialize).toHaveBeenCalledTimes(2);
    expect(h.commits[1]).toMatchObject({ origin: { revision: 'r8' }, current: true, association: { etag: 'etag-2' } });
  });

  it('explicit Save joins autosave without overlapping writes or reusing a stale token', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2');
    await vi.advanceTimersByTimeAsync(750);
    expect(local.save).toHaveBeenCalledOnce();
    h.edit('r3');
    const manual = h.controller.flush();
    const repeat = h.controller.flush();
    expect(local.save).toHaveBeenCalledOnce();
    local.finish(0, 'etag-1');
    await ticks();
    expect(local.pending[1]?.expected).toBe('etag-1');
    expect(local.save).toHaveBeenCalledTimes(2);
    local.finish(1, 'etag-2');
    expect(await manual).toBe(true);
    expect(await repeat).toBe(true);
    expect(await h.controller.flush()).toBe(true);
    expect(local.save).toHaveBeenCalledTimes(2);
  });

  it('bounds a drain to its original save and one latest follow-up', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2');
    const saving = h.controller.flush();
    await ticks();
    h.edit('r3');
    await vi.advanceTimersByTimeAsync(750);
    local.finish(0, 'etag-1');
    await ticks();
    h.edit('r4');
    await vi.advanceTimersByTimeAsync(750);
    local.finish(1, 'etag-2');
    expect(await saving).toBe(false);
    expect(local.save).toHaveBeenCalledTimes(2);
    expect(h.controller.state.dirty).toBe(true);
    await vi.advanceTimersByTimeAsync(750);
    expect(local.save).toHaveBeenCalledTimes(3);
    expect(local.pending[2]?.expected).toBe('etag-2');
    local.finish(2, 'etag-3');
    await ticks();
    expect(h.controller.state.dirty).toBe(false);
  });

  it('reopening the same document in a new session obsoletes old status and token callbacks', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.controller.reset({ sessionId: 'session-a', id: 'document-a', etag: 'etag-before' });
    h.edit('r2');
    const old = h.controller.flush();
    await ticks();
    h.set({ sessionId: 'session-b', revision: 'new-r1' }, false);
    h.controller.reset({ sessionId: 'session-b', id: 'document-a', etag: 'loaded-current-token' });
    h.edit('new-r2');
    const current = h.controller.flush();
    local.finish(0, 'obsolete-token');
    await ticks();
    expect(h.commits).toEqual([]);
    expect(local.pending[1]?.expected).toBe('loaded-current-token');
    local.finish(1, 'new-token');
    expect(await old).toBe(false);
    expect(await current).toBe(true);
    expect(h.commits).toHaveLength(1);
    expect(h.commits[0]?.association.sessionId).toBe('session-b');
  });

  it('an obsolete failed save cannot pause or report an error on a newly opened document', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2');
    const old = h.controller.flush();
    await ticks();
    h.set({ sessionId: 'session-b', documentId: 'document-b', revision: 'new-r1' }, false);
    h.controller.reset(null);
    h.edit('new-r2');
    const current = h.controller.flush();
    local.pending[0]?.done.reject(new StorageError('quota', 'Storage full'));
    await ticks();
    expect(h.controller.state.error).toBeNull();
    expect(local.pending[1]?.expected).toBeNull();
    local.finish(1, 'new-token');
    expect(await old).toBe(false);
    expect(await current).toBe(true);
    expect(h.controller.state).toMatchObject({ phase: 'saved', error: null });
  });

  it('an explicit rebind invalidates old completions even when all origin strings coincide', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2');
    const old = h.controller.flush();
    await ticks();
    h.controller.reset({ sessionId: 'session-a', id: 'document-a', etag: 'reopened-token' });
    local.finish(0, 'obsolete-token');
    expect(await old).toBe(false);
    expect(h.commits).toEqual([]);
    expect(h.controller.state).toMatchObject({ phase: 'saved', dirty: false, savedLocally: true });
    h.edit('r3');
    const current = h.controller.flush();
    await ticks();
    expect(local.pending[1]?.expected).toBe('reopened-token');
    local.finish(1, 'current-token');
    expect(await current).toBe(true);
  });

  it('pauses automatic saving after failures, retaining edits until explicit retry', async () => {
    const error = new StorageError('quota', 'Storage full');
    const save = vi.fn<WriterStorage['save']>().mockRejectedValueOnce(error)
      .mockImplementation(async (value) => stored(value, 'retry-token'));
    const h = harness({ save });
    h.edit('r2');
    await vi.advanceTimersByTimeAsync(750);
    expect(h.controller.state).toMatchObject({ phase: 'error', dirty: true, error });
    h.edit('r3');
    await vi.advanceTimersByTimeAsync(10_000);
    expect(await h.controller.flushPending()).toBe(false);
    expect(save).toHaveBeenCalledOnce();
    expect(await h.controller.flush()).toBe(true);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1]?.[0].bytes).toEqual(new TextEncoder().encode('r3'));
    expect(h.controller.state).toMatchObject({ phase: 'saved', dirty: false, error: null });
  });

  it('does not report Saved or flush while a pristine saved document has active IME work', async () => {
    const h = harness();
    h.controller.reset({ sessionId: 'session-a', id: 'document-a', etag: 'existing' });
    h.controller.setComposing(true);
    expect(h.controller.state).toMatchObject({ phase: 'pending', dirty: true, composing: true });
    expect(await h.controller.flush()).toBe(false);
    expect(await h.controller.flushPending()).toBe(false);
    await vi.advanceTimersByTimeAsync(2_000);
    expect(h.storage.save).not.toHaveBeenCalled();
    h.controller.setComposing(false);
    expect(h.controller.state).toMatchObject({ phase: 'saved', dirty: false, composing: false });
    expect(h.storage.save).not.toHaveBeenCalled();
  });

  it('autosaves committed composition once after suspension ends', async () => {
    const h = harness();
    h.controller.setComposing(true);
    h.edit('r2', '日本語 👩🏽‍💻');
    await vi.advanceTimersByTimeAsync(2_000);
    expect(h.storage.save).not.toHaveBeenCalled();
    h.controller.setComposing(false);
    await vi.advanceTimersByTimeAsync(750);
    expect(h.storage.save).toHaveBeenCalledOnce();
    expect(h.storage.save).toHaveBeenCalledWith(expect.objectContaining({ bytes: new TextEncoder().encode('日本語 👩🏽‍💻') }), null);
    expect(h.controller.state.dirty).toBe(false);
  });

  it('an earlier save completing during IME cannot clear visible unsaved composition', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2');
    const saved = h.controller.flush();
    await ticks();
    h.controller.setComposing(true);
    local.finish(0, 'saved-core');
    expect(await saved).toBe(false);
    expect(h.commits[0]?.current).toBe(false);
    expect(h.controller.state).toMatchObject({ phase: 'pending', dirty: true, composing: true });
    h.controller.setComposing(false);
    expect(h.controller.state).toMatchObject({ phase: 'saved', dirty: false });
  });

  it('serialization failures pause retries without invoking storage or changing the live source', async () => {
    const h = harness();
    h.edit('r2');
    const original = h.live.bytes.slice();
    h.serialize.mockImplementationOnce(() => { throw new Error('Engine refused serialization'); });
    expect(await h.controller.flush()).toBe(false);
    expect(h.storage.save).not.toHaveBeenCalled();
    expect(h.live.bytes).toEqual(original);
    await vi.advanceTimersByTimeAsync(2_000);
    expect(h.serialize).toHaveBeenCalledOnce();
    expect(h.controller.state).toMatchObject({ phase: 'error', dirty: true });
    expect(await h.controller.flush()).toBe(true);
  });

  it('cancel/dispose prevents delayed writes and obsolete callbacks while leaving active storage alone', async () => {
    const local = controlledStorage();
    const h = harness(local);
    h.edit('r2');
    h.controller.cancelPending();
    await vi.advanceTimersByTimeAsync(2_000);
    expect(local.save).not.toHaveBeenCalled();
    const saved = h.controller.flush();
    await ticks();
    const states = h.states.length;
    h.controller.dispose();
    h.current.mockImplementation(() => { throw new Error('Disposed engine must not be read'); });
    local.finish(0, 'finished-after-dispose');
    expect(await saved).toBe(false);
    expect(h.states).toHaveLength(states);
    expect(h.commits).toEqual([]);
    expect(await h.controller.flush()).toBe(false);
  });

  it('rejects a save association from another session before replacing the current binding', () => {
    const h = harness();
    expect(() => h.controller.reset({ sessionId: 'foreign', id: 'document-a', etag: 'token' })).toThrow(StorageError);
    expect(h.controller.state).toMatchObject({ savedLocally: false, phase: 'idle' });
  });

  it('round trips exact native bytes and metadata through the existing IndexedDB transaction layer', async () => {
    const library = new WriterStorage('writer-autosave-roundtrip');
    libraries.push(library);
    const h = harness(library);
    // The controller treats the committed native fixture as opaque bytes. It
    // does not decode a second document model in order to save it.
    const native = new Uint8Array(readFileSync(new URL('../../../tests/fixtures/layout/layout-v1.writer', import.meta.url)));
    h.set({ documentId: 'layout-reference-document', title: 'Typography reference', revision: 'r2', bytes: native });
    expect(await h.controller.flush()).toBe(true);
    const record = await library.load(h.live.documentId);
    expect(record.title).toBe(h.live.title);
    expect(record.bytes).toEqual(h.live.bytes);
    const renamed = new TextEncoder().encode(new TextDecoder().decode(native).replace('Typography reference', 'Writer — 文書'));
    h.set({ title: 'Writer — 文書', revision: 'r3', bytes: renamed });
    expect(await h.controller.flush()).toBe(true);
    const next = await library.load(h.live.documentId);
    expect(next.bytes).toEqual(h.live.bytes);
    expect(next.etag).not.toBe(record.etag);
  });

  it('protects another tab’s save, pauses on conflict, and can save a distinct fork', async () => {
    const library = new WriterStorage('writer-autosave-conflict');
    libraries.push(library);
    const first = harness(library);
    first.edit('r2', 'first saved record');
    expect(await first.controller.flush()).toBe(true);
    const earlier = await library.load(first.live.documentId);
    const second = harness(library);
    second.set({ sessionId: 'other-tab' }, false);
    second.controller.reset({ sessionId: 'other-tab', id: earlier.id, etag: earlier.etag });
    first.edit('r3', 'newer first-tab record');
    expect(await first.controller.flush()).toBe(true);
    second.edit('other-r2', 'unsaved second-tab edits');
    expect(await second.controller.flush()).toBe(false);
    expect(second.controller.state).toMatchObject({ phase: 'error', dirty: true, error: { code: 'conflict' } });
    expect((await library.load(earlier.id)).bytes).toEqual(first.live.bytes);
    second.edit('other-r3', 'more second-tab edits');
    await vi.advanceTimersByTimeAsync(2_000);
    expect((await library.load(earlier.id)).bytes).toEqual(first.live.bytes);
    second.set({ documentId: 'fork-id', sessionId: 'fork-session', revision: 'fork-r1' }, false);
    second.controller.reset(null);
    second.edit('fork-r2', 'safe fork bytes');
    expect(await second.controller.flush()).toBe(true);
    expect((await library.load('fork-id')).bytes).toEqual(second.live.bytes);
    expect((await library.load(earlier.id)).bytes).toEqual(first.live.bytes);
  });

  it('a reliable IndexedDB write failure preserves the previous valid file and supports explicit retry', async () => {
    const library = new WriterStorage('writer-autosave-quota');
    libraries.push(library);
    const h = harness(library);
    h.edit('r2', 'valid saved file');
    expect(await h.controller.flush()).toBe(true);
    const previous = await library.load(h.live.documentId);
    const put = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(() => {
      throw new DOMException('Storage quota exceeded', 'QuotaExceededError');
    });
    h.edit('r3', 'unsaved current work');
    expect(await h.controller.flush()).toBe(false);
    expect(h.controller.state).toMatchObject({ phase: 'error', dirty: true, error: { code: 'quota' } });
    expect((await library.load(h.live.documentId))).toEqual(previous);
    put.mockRestore();
    expect(await h.controller.flush()).toBe(true);
    expect((await library.load(h.live.documentId)).bytes).toEqual(h.live.bytes);
  });
});
