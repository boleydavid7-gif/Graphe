/** Only the embedding ProairetOS origin can receive document bytes.
 * No file is sent until the user explicitly selects Save to ProairetOS. */
const HOSTS = new Set(['https://proairetos.com', 'https://research-and-development-proairetos.boleydavid7.workers.dev']);
const MAX_BYTES = 10 * 1024 * 1024;
let connectedHost: string | null = null;
export type FilesOpenPayload = { fileId: string; name: string; mime: string; data: ArrayBuffer };
const pending = new Map<string, { resolve: (value: string) => void; reject: (reason: Error) => void }>();
const supported = ['writer', 'docx', 'odt', 'rtf', 'txt', 'pdf'];
const MIME: Record<string, string> = {
  writer: 'application/json', docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  odt: 'application/vnd.oasis.opendocument.text', rtf: 'application/rtf', txt: 'text/plain', pdf: 'application/pdf',
};

export function installProairetosFilesBridge(): () => void {
  if (window.parent === window) return () => {};
  const receive = (event: MessageEvent) => {
    if (event.source !== window.parent || !HOSTS.has(event.origin)) return;
    if (event.data?.type === 'proairetos:lamplight' && event.data.version === 1 && event.data.app === 'writer') {
      connectedHost = event.origin;
      window.parent.postMessage({ type: 'writer:files-ready', version: 1 }, connectedHost);
    }
    if (event.origin !== connectedHost || event.data?.version !== 1) return;
    if (event.data?.type === 'proairetos:files-open') {
      const payload = event.data as Partial<FilesOpenPayload>;
      if (typeof payload.fileId !== 'string' || !/^[a-zA-Z0-9_-]{1,160}$/.test(payload.fileId)
        || typeof payload.name !== 'string' || payload.name.length > 120
        || !/\.(writer|docx|odt|rtf|txt)$/i.test(payload.name)
        || typeof payload.mime !== 'string' || !(payload.data instanceof ArrayBuffer)
        || payload.data.byteLength === 0 || payload.data.byteLength > 5 * 1024 * 1024) return;
      window.dispatchEvent(new CustomEvent('proairetos:files-open', { detail: payload }));
      return;
    }
    if (event.data?.type !== 'proairetos:files-save-result') return;
    const id = event.data.requestId;
    if (typeof id !== 'string') return;
    const waiting = pending.get(id);
    if (!waiting) return;
    pending.delete(id);
    if (event.data.ok === true && typeof event.data.fileId === 'string' && /^[a-zA-Z0-9_-]{1,160}$/.test(event.data.fileId)) waiting.resolve(event.data.fileId);
    else waiting.reject(new Error(typeof event.data.message === 'string' ? event.data.message : 'Save was cancelled.'));
  };
  window.addEventListener('message', receive);
  return () => {
    window.removeEventListener('message', receive);
    connectedHost = null;
    for (const request of pending.values()) request.reject(new Error('ProairetOS connection closed.'));
    pending.clear();
  };
}

export function announceFilesReady(): void {
  if (connectedHost) window.parent.postMessage({ type: 'writer:files-ready', version: 1 }, connectedHost);
}

export function canSaveToProairetos(): boolean { return connectedHost !== null && window.parent !== window; }

export function saveToProairetos(name: string, format: string, data: Uint8Array, fileId?: string): Promise<string> {
  if (!connectedHost || !supported.includes(format) || !data.byteLength || data.byteLength > MAX_BYTES)
    return Promise.reject(new Error('ProairetOS Files is unavailable or the document exceeds 10 MB.'));
  if (pending.size) return Promise.reject(new Error('A Files save is already in progress.'));
  const requestId = crypto.randomUUID().replace(/-/g, '');
  const bytes = data.slice().buffer;
  return new Promise((resolve, reject) => {
    pending.set(requestId, { resolve, reject });
    window.parent.postMessage({
      type: 'writer:files-save-request', version: 1, requestId, name, mime: MIME[format], data: bytes, ...(fileId ? { fileId } : {}),
    }, connectedHost!, [bytes]);
  });
}
