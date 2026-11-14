import { useId, useMemo, useRef, useState } from 'react';
import type { FindSnapshot, WriterCommand, WriterView } from './types';

interface FindBarProps {
  readonly view: WriterView;
  readonly onFind: (query: string) => FindSnapshot;
  readonly onCommand: (command: WriterCommand) => WriterView | undefined;
  readonly onClose: () => void;
  readonly onFocusDocument: () => void;
  readonly onRevealSelection?: (view: WriterView) => void;
}

/** Literal search results are Rust snapshots; only query/navigation UI lives here. */
export function FindBar({ view, onFind, onCommand, onClose, onFocusDocument, onRevealSelection }: FindBarProps) {
  const [query, setQuery] = useState('');
  const [navigation, setNavigation] = useState({ key: '', index: -1 });
  const input = useRef<HTMLInputElement>(null);
  const description = useId();
  const key = JSON.stringify([query, view.sessionId, view.documentId, view.revision]);
  const state = useMemo(() => {
    try {
      const result = onFind(query);
      if (result.sessionId !== view.sessionId || result.documentId !== view.documentId || result.revision !== view.revision) {
        throw new Error('The document changed. Try the search again.');
      }
      return { result, error: '' };
    } catch (failure) {
      return { result: undefined, error: failure instanceof Error ? failure.message : 'Find could not be completed.' };
    }
  }, [onFind, query, view.documentId, view.revision, view.sessionId]);
  const matches = state.result?.matches ?? [];
  const index = navigation.key === key ? navigation.index : -1;

  function move(delta: number) {
    if (!state.result || !matches.length) return;
    const nextIndex = index < 0 ? (delta < 0 ? matches.length - 1 : 0)
      : (index + delta + matches.length) % matches.length;
    const match = matches[nextIndex];
    if (!match) return;
    const next = onCommand({ type: 'set_selection', selection: match.selection, expectedRevision: state.result.revision });
    if (next?.sessionId === state.result.sessionId && next.documentId === state.result.documentId) {
      setNavigation({ key, index: nextIndex });
      onRevealSelection?.(next);
    }
  }

  function close() { onClose(); onFocusDocument(); }

  return <form className="find-bar" role="search" aria-label="Find in document" onSubmit={(event) => {
    event.preventDefault(); move(1);
  }} onKeyDown={(event) => {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); }
    if (event.key === 'Enter' && event.shiftKey) { event.preventDefault(); move(-1); }
  }}>
    <label className="visually-hidden" htmlFor={description}>Find text</label>
    <input id={description} ref={input} className="find-input" data-writer-find-input aria-label="Find text" placeholder="Find in document" autoFocus
      value={query} maxLength={256} aria-describedby={`${description}-help`}
      onChange={(event) => setQuery(event.target.value)} />
    <span className="find-summary" role="status" aria-live="polite">{!query ? 'Exact text'
      : state.error ? 'Find unavailable'
      : matches.length === 0 ? 'No matches'
      : `${index < 0 ? '—' : index + 1} of ${matches.length}${state.result?.truncated ? '+' : ''}`}</span>
    <button type="button" aria-label="Previous match" title="Previous match (Shift Enter)" disabled={!matches.length}
      onClick={() => move(-1)}>↑</button>
    <button type="submit" aria-label="Next match" title="Next match (Enter)" disabled={!matches.length}>↓</button>
    <button type="button" aria-label="Close find" onClick={close}>×</button>
    <span id={`${description}-help`} className="visually-hidden">Case-sensitive exact text within paragraphs. Matches select complete Unicode graphemes. Enter selects the next match; Escape returns to the document.</span>
    {state.error && <span className="find-error" role="alert">{state.error}</span>}
  </form>;
}
