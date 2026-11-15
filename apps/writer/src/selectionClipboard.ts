import type { WriterCommand, WriterSelection, WriterView } from './types';

export type SelectionClipboardAction = 'copy' | 'cut' | 'paste';

export interface SelectionClipboard {
  readonly writeText?: (text: string) => Promise<void>;
  readonly readText?: () => Promise<string>;
}

export interface SelectionClipboardOptions {
  readonly clipboard: SelectionClipboard | undefined;
  /** Read the live Rust snapshot, including same-revision selection changes. */
  readonly getCurrentView: () => WriterView | null;
  /** Includes composition, replacement/disabled state and action-bar lifetime. */
  readonly canInteract: () => boolean;
  readonly onCopy: () => string;
  readonly onCommand: (command: WriterCommand) => WriterView | undefined;
}

function sameSelection(left: WriterSelection | null, right: WriterSelection | null): boolean {
  return left === right || (!!left && !!right
    && left.anchor.paragraphId === right.anchor.paragraphId && left.anchor.utf16 === right.anchor.utf16
    && left.focus.paragraphId === right.focus.paragraphId && left.focus.utf16 === right.focus.utf16);
}

/**
 * Browser clipboard I/O around a captured Rust selection. No text positions or
 * edits are computed here; destructive completions return through Rust only.
 */
export async function selectionClipboardAction(
  action: SelectionClipboardAction, options: SelectionClipboardOptions,
): Promise<'copied' | 'cut' | 'pasted' | 'not-applied'> {
  const view = options.getCurrentView();
  if (!view || !options.canInteract()) {
    throw new Error('Finish the current input or document operation before using the clipboard.');
  }
  const captured = {
    sessionId: view.sessionId, documentId: view.documentId, revision: view.revision,
    selection: view.selection ? { anchor: { ...view.selection.anchor }, focus: { ...view.selection.focus } } : null,
  };
  const selected = captured.selection && (captured.selection.anchor.paragraphId !== captured.selection.focus.paragraphId
    || captured.selection.anchor.utf16 !== captured.selection.focus.utf16);
  if (action !== 'paste' && !selected) throw new Error('Select text before copying or cutting.');

  const stillCurrent = () => {
    const current = options.getCurrentView();
    return options.canInteract() && !!current && captured.sessionId === current.sessionId
      && captured.documentId === current.documentId && captured.revision === current.revision
      && sameSelection(captured.selection, current.selection);
  };

  let text: string;
  if (action === 'paste') {
    if (!options.clipboard?.readText) {
      throw new Error('Clipboard reading is unavailable. Use the keyboard or device paste command.');
    }
    try { text = await options.clipboard.readText(); }
    catch { throw new Error('The browser could not read clipboard text. Use the keyboard or device paste command.'); }
    if (!text) throw new Error('The clipboard has no text to paste. Your document is unchanged.');
  } else {
    if (!options.clipboard?.writeText) {
      throw new Error('Clipboard writing is unavailable. Use the keyboard or device copy command.');
    }
    // Capture Rust's exact structural plain text before yielding to the browser.
    text = options.onCopy();
    try { await options.clipboard.writeText(text); }
    catch { throw new Error('The browser could not copy the selected text. Nothing was cut.'); }
    if (action === 'copy') return 'copied';
  }

  if (!stillCurrent()) {
    throw new Error(`The document or selection changed while accessing the clipboard. Nothing was ${action === 'cut' ? 'cut' : 'pasted'}. Try again.`);
  }
  const next = options.onCommand({ type: 'replace_text', text: action === 'cut' ? '' : text,
    expectedRevision: captured.revision, ...(captured.selection ? { selection: captured.selection } : {}) });
  // The existing command boundary reports structured Rust failures. Do not
  // replace its diagnostic or announce success when Rust rejected the edit.
  return next ? action === 'cut' ? 'cut' : 'pasted' : 'not-applied';
}
