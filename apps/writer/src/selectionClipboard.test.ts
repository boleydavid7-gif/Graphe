import { describe, expect, it, vi } from 'vitest';
import { selectionClipboardAction, type SelectionClipboardOptions } from './selectionClipboard';
import type { ParagraphView, WriterCommand, WriterView } from './types';

const selectedText = 'Café e\u0301 👩🏽‍💻\nالعربية';

function paragraph(id: string, text: string): ParagraphView {
  return { id, sectionId: 'section', styleId: null, list: null,
    runs: [{ text, format: { bold: false, italic: false, underline: 'none', fontFamily: 'Arimo', fontSizePt: 12, color: '#000000', hyperlink: null } }],
    format: { alignment: 'left', spacingBeforePt: 0, spacingAfterPt: 0, lineHeight: 'normal',
      indentLeftPt: 0, indentRightPt: 0, firstLinePt: 0 } };
}

function view(): WriterView {
  return { sessionId: 'session', documentId: 'document', revision: '1', title: 'Clipboard test',
    paragraphs: [paragraph('first', 'Café e\u0301 👩🏽‍💻'), paragraph('second', 'العربية')], styles: [],
    selection: { anchor: { paragraphId: 'first', utf16: 0 }, focus: { paragraphId: 'second', utf16: 7 } },
    format: { bold: false, italic: false, underline: false, fontFamily: 'Arimo', fontSizePt: 12,
      alignment: 'left', styleId: null, styleMixed: false, list: null, listMixed: false, hyperlink: null },
    canUndo: true, canRedo: false, wordCount: 5, originalSource: null };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

// This substitutes only browser I/O and the Rust command boundary. It does not
// edit strings or maintain a document/history model in the test adapter.
function setup() {
  let current: WriterView | null = view();
  let available = true;
  const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue(undefined);
  const readText = vi.fn<() => Promise<string>>().mockResolvedValue('Pasted e\u0301 👩🏽‍💻\nनमस्ते');
  const onCopy = vi.fn(() => selectedText);
  const onCommand = vi.fn<(command: WriterCommand) => WriterView | undefined>(() => current ?? undefined);
  const options: SelectionClipboardOptions = {
    clipboard: { writeText, readText }, getCurrentView: () => current, canInteract: () => available,
    onCopy, onCommand,
  };
  return { options, writeText, readText, onCopy, onCommand,
    current: () => current!, replace: (next: WriterView | null) => { current = next; },
    block: () => { available = false; } };
}

describe('selection clipboard safety across asynchronous browser I/O', () => {
  it('cuts only after clipboard completion and sends the exact captured Rust selection/revision', async () => {
    const state = setup();
    const pending = deferred<void>();
    state.writeText.mockReturnValue(pending.promise);
    const before = structuredClone(state.current());
    const result = selectionClipboardAction('cut', state.options);
    expect(state.writeText).toHaveBeenCalledWith(selectedText);
    expect(state.onCommand).not.toHaveBeenCalled();
    pending.resolve();
    await expect(result).resolves.toBe('cut');
    expect(state.onCommand).toHaveBeenCalledExactlyOnceWith({ type: 'replace_text', text: '',
      expectedRevision: before.revision, selection: before.selection });
    expect(state.current()).toEqual(before);
  });

  it('never cuts after a rejected browser write', async () => {
    const state = setup();
    state.writeText.mockRejectedValue(new Error('Permission denied'));
    await expect(selectionClipboardAction('cut', state.options)).rejects.toThrow('Nothing was cut');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('rejects a same-revision selection change while copying before a cut', async () => {
    const state = setup();
    const pending = deferred<void>();
    state.writeText.mockReturnValue(pending.promise);
    const result = selectionClipboardAction('cut', state.options);
    state.replace({ ...state.current(), selection: { anchor: { paragraphId: 'first', utf16: 5 },
      focus: { paragraphId: 'first', utf16: 7 } } });
    pending.resolve();
    await expect(result).rejects.toThrow('selection changed');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it.each(['sessionId', 'documentId', 'revision'] as const)('rejects changed %s before a destructive completion', async (field) => {
    const state = setup();
    const pending = deferred<void>();
    state.writeText.mockReturnValue(pending.promise);
    const result = selectionClipboardAction('cut', state.options);
    state.replace({ ...state.current(), [field]: 'new-value' });
    pending.resolve();
    await expect(result).rejects.toThrow('Nothing was cut');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('rejects if composition, replacement or dismissal blocks interaction during I/O', async () => {
    const state = setup();
    const pending = deferred<string>();
    state.readText.mockReturnValue(pending.promise);
    const result = selectionClipboardAction('paste', state.options);
    state.block();
    pending.resolve('Clipboard text');
    await expect(result).rejects.toThrow('Nothing was pasted');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('rejects a disposed session after clipboard access', async () => {
    const state = setup();
    const pending = deferred<string>();
    state.readText.mockReturnValue(pending.promise);
    const result = selectionClipboardAction('paste', state.options);
    state.replace(null);
    pending.resolve('Clipboard text');
    await expect(result).rejects.toThrow('Nothing was pasted');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('pastes Unicode and structural newlines through one existing Rust command', async () => {
    const state = setup();
    await expect(selectionClipboardAction('paste', state.options)).resolves.toBe('pasted');
    expect(state.onCommand).toHaveBeenCalledExactlyOnceWith({ type: 'replace_text',
      text: 'Pasted e\u0301 👩🏽‍💻\nनमस्ते', selection: state.current().selection, expectedRevision: '1' });
    expect(state.onCopy).not.toHaveBeenCalled();
  });

  it('does not erase a selection when the clipboard returns empty text', async () => {
    const state = setup();
    state.readText.mockResolvedValue('');
    await expect(selectionClipboardAction('paste', state.options)).rejects.toThrow('no text to paste');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('does not paste at an old selection after asynchronous permission or clipboard prompts', async () => {
    const state = setup();
    const pending = deferred<string>();
    state.readText.mockReturnValue(pending.promise);
    const result = selectionClipboardAction('paste', state.options);
    state.replace({ ...state.current(), selection: { anchor: { paragraphId: 'second', utf16: 7 },
      focus: { paragraphId: 'second', utf16: 7 } } });
    pending.resolve('Clipboard text');
    await expect(result).rejects.toThrow('Nothing was pasted');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('does not claim success when Rust rejects the edit', async () => {
    const state = setup();
    state.onCommand.mockReturnValue(undefined);
    await expect(selectionClipboardAction('paste', state.options)).resolves.toBe('not-applied');
  });

  it('copies exact Rust text without dispatching a mutation', async () => {
    const state = setup();
    await expect(selectionClipboardAction('copy', state.options)).resolves.toBe('copied');
    expect(state.writeText).toHaveBeenCalledExactlyOnceWith(selectedText);
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('preserves the document when clipboard reading is denied', async () => {
    const state = setup();
    state.readText.mockRejectedValue(new Error('Permission denied'));
    await expect(selectionClipboardAction('paste', state.options)).rejects.toThrow('could not read');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('rejects unsupported clipboard APIs rather than treating them as completed', async () => {
    const state = setup();
    await expect(selectionClipboardAction('copy', { ...state.options, clipboard: undefined })).rejects.toThrow('unavailable');
    await expect(selectionClipboardAction('paste', { ...state.options, clipboard: {} })).rejects.toThrow('unavailable');
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('does not begin clipboard access during composition or a disabled operation', async () => {
    const state = setup();
    state.block();
    await expect(selectionClipboardAction('cut', state.options)).rejects.toThrow('Finish the current input');
    expect(state.writeText).not.toHaveBeenCalled();
    expect(state.onCopy).not.toHaveBeenCalled();
    expect(state.onCommand).not.toHaveBeenCalled();
  });

  it('does not copy or cut a collapsed Rust selection', async () => {
    const state = setup();
    state.replace({ ...state.current(), selection: { anchor: { paragraphId: 'first', utf16: 0 },
      focus: { paragraphId: 'first', utf16: 0 } } });
    await expect(selectionClipboardAction('cut', state.options)).rejects.toThrow('Select text');
    expect(state.writeText).not.toHaveBeenCalled();
    expect(state.onCommand).not.toHaveBeenCalled();
  });
});
