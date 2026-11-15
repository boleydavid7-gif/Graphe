import { useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';
import { selectionClipboardAction, type SelectionClipboardAction } from './selectionClipboard';
import type { WriterCommand, WriterView } from './types';

export interface SelectionActionsProps {
  readonly view: WriterView;
  readonly disabled?: boolean;
  readonly composing?: boolean;
  readonly getCurrentView: () => WriterView | null;
  readonly canInteract: () => boolean;
  readonly onCopy: () => string;
  readonly onCommand: (command: WriterCommand) => WriterView | undefined;
  readonly onError: (message: string) => void;
  readonly onFocusEditor?: () => void;
}

/** Clipboard controls for a Rust selection, independent of native SVG callouts. */
export function SelectionActions(props: SelectionActionsProps) {
  const latest = useRef(props);
  const mounted = useRef(true);
  const running = useRef(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  useLayoutEffect(() => { latest.current = props; });
  useLayoutEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const clipboard = typeof navigator === 'undefined' ? undefined : navigator.clipboard;
  const selection = props.view.selection;
  const selected = selection && (selection.anchor.paragraphId !== selection.focus.paragraphId
    || selection.anchor.utf16 !== selection.focus.utf16);
  const unavailable = !!props.disabled || !!props.composing || busy;
  const canWrite = typeof clipboard?.writeText === 'function';
  const canRead = typeof clipboard?.readText === 'function';

  async function perform(action: SelectionClipboardAction) {
    if (running.current || unavailable) return;
    running.current = true;
    setBusy(true);
    setNotice('');
    try {
      const result = await selectionClipboardAction(action, {
        // Keep Clipboard's browser receiver when delegating its methods.
        clipboard: clipboard ? {
          writeText: canWrite ? (text) => clipboard.writeText(text) : undefined,
          readText: canRead ? () => clipboard.readText() : undefined,
        } : undefined,
        getCurrentView: () => latest.current.getCurrentView(),
        canInteract: () => mounted.current && !latest.current.disabled && !latest.current.composing
          && latest.current.canInteract(),
        onCopy: () => latest.current.onCopy(),
        onCommand: (command) => latest.current.onCommand(command),
      });
      if (mounted.current && result !== 'not-applied') {
        setNotice(result === 'copied' ? 'Copied selected text.' : result === 'cut' ? 'Cut selected text.' : 'Pasted clipboard text.');
      }
    } catch (error) {
      if (mounted.current) latest.current.onError(error instanceof Error ? error.message : 'The clipboard action could not be completed.');
    } finally {
      running.current = false;
      if (mounted.current) setBusy(false);
    }
  }

  function navigate(event: KeyboardEvent<HTMLDivElement>) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    const controls = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
    if (!controls.length) return;
    const index = controls.findIndex((control) => control === document.activeElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? controls.length - 1
      : (index + (event.key === 'ArrowLeft' ? -1 : 1) + controls.length) % controls.length;
    event.preventDefault();
    controls[next]?.focus({ preventScroll: true });
  }

  return <div className="selection-actions" role="toolbar" aria-label="Text selection"
    aria-busy={busy || undefined} onKeyDown={navigate}>
    <button type="button" disabled={unavailable || !selected || !canWrite}
      title={canWrite ? 'Copy selected text' : 'Clipboard access is unavailable; use the keyboard or device copy command.'}
      onMouseDown={(event) => event.preventDefault()} onClick={() => void perform('copy')}>Copy</button>
    <button type="button" disabled={unavailable || !selected || !canWrite}
      title={canWrite ? 'Cut selected text' : 'Clipboard access is unavailable; use the keyboard or device cut command.'}
      onMouseDown={(event) => event.preventDefault()} onClick={() => void perform('cut')}>Cut</button>
    <button type="button" disabled={unavailable || !canRead}
      title={canRead ? 'Paste clipboard text' : 'Clipboard reading is unavailable; use the keyboard or device paste command.'}
      onMouseDown={(event) => event.preventDefault()} onClick={() => void perform('paste')}>Paste</button>
    {props.onFocusEditor && <button type="button" disabled={unavailable}
      onMouseDown={(event) => event.preventDefault()} onClick={() => {
        if (latest.current.canInteract()) latest.current.onFocusEditor?.();
      }}>Edit selected text</button>}
    {notice && <span className="selection-actions-status" role="status">{notice}</span>}
  </div>;
}
