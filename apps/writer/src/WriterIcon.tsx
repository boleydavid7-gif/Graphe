/** Small local interface icons. Document glyphs are rendered separately by Rust. */
export function WriterIcon({ name, className = '' }: {
  name: 'lantern' | 'new' | 'open' | 'save' | 'search' | 'more' | 'text-format' | 'list-bullet' | 'list-numbered' | 'undo' | 'redo' | 'export' | 'minus' | 'plus';
  className?: string;
}) {
  const paths: Record<typeof name, string> = {
    lantern: 'M12 2v3m-5 1 5-2 5 2m-13 2h16M7 8v11m10-11v11M6 19h12M8 22h8M9 8l-2 6 5 5 5-5-2-6M9 11v5m6-5v5',
    new: 'M6 3h8l4 4v14H6zM14 3v5h4M9 14h6m-3-3v6',
    open: 'M3 7V5h7l2 2h9v13H3V7zm0 3h18',
    save: 'M4 3h14l3 3v15H3V3h1zm3 0v6h10V3M7 21v-8h10v8M14 5v2',
    search: 'M16 16l5 5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',
    more: 'M5 12h.01M12 12h.01M19 12h.01',
    'text-format': 'M2.5 19 8 5l5.5 14M4.8 13.5h6.4M16 7h5M16 12h5M16 17h5',
    'list-bullet': 'M4 6h.01M4 12h.01M4 18h.01M9 6h11M9 12h11M9 18h11',
    'list-numbered': 'M3 5h1v4M2.5 13c0-1 3-1 3 0s-3 2-3 3h3M3 19h2m-2 2h2M9 6h11M9 12h11M9 18h11',
    undo: 'm8 4-5 5 5 5M3 9h11a6 6 0 0 1 0 12',
    redo: 'm16 4 5 5-5 5m5-5H10a6 6 0 0 0 0 12',
    export: 'M12 16V2m-5 5 5-5 5 5M5 11H3v10h18V11h-2',
    minus: 'M5 12h14',
    plus: 'M5 12h14M12 5v14',
  };
  return <svg aria-hidden="true" focusable="false" className={className}
    width="22" height="22" viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth={name === 'more' ? 3 : 1.6}
    strokeLinecap="round" strokeLinejoin="round"><path d={paths[name]} /></svg>;
}
