import { useLayoutEffect, useRef, useState, type CSSProperties, type MouseEvent } from 'react';

export interface PreviewChoice {
  value: string;
  label: string;
  sample: string;
  group: string;
  font: string;
  size: number;
  bold?: boolean;
  note?: string;
}

/** Controls only the choice UI. Document edits remain with Graphe's Rust command adapter. */
export function VisualPicker({ title, triggerLabel, triggerFont, items, selected, onChoose, disabled = false }: {
  title: string;
  triggerLabel: string;
  triggerFont?: string;
  items: readonly PreviewChoice[];
  selected: string;
  onChoose: (value: string) => void;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState({ x: 12, y: 64 });
  const dialog = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => {
    const element = dialog.current;
    if (!open || !element) return;
    element.showModal();
    return () => { if (element.open) element.close(); };
  }, [open]);
  function show(event: MouseEvent<HTMLButtonElement>) {
    const rect = event.currentTarget.getBoundingClientRect();
    const width = Math.min(370, window.innerWidth - 16);
    const height = Math.min(510, window.innerHeight * .72);
    setAnchor({
      x: Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(rect.bottom + 8, window.innerHeight - height - 8)),
    });
    setOpen(true);
  }
  function choose(value: string) {
    onChoose(value);
    setOpen(false);
  }
  return <>
    <button className="writer-preview-trigger" type="button" disabled={disabled}
      aria-haspopup="dialog" aria-expanded={open} aria-label={'Browse ' + title.toLowerCase() + ' with previews'}
      onMouseDown={(event) => event.preventDefault()} onClick={show}>
      <span className="writer-preview-trigger-label"
        style={triggerFont ? { fontFamily: '"' + triggerFont + '", sans-serif' } : undefined}>
        {triggerLabel}
      </span>
      <span aria-hidden="true" className="writer-preview-trigger-chevron">⌄</span>
    </button>
    {open && <dialog ref={dialog} className="writer-visual-choice"
      aria-label={title === 'Fonts' ? 'Font previews' : 'Style previews'}
      style={{ '--picker-x': anchor.x + 'px', '--picker-y': anchor.y + 'px' } as CSSProperties}
      onCancel={(event) => { event.preventDefault(); setOpen(false); }}
      onClick={(event) => { if (event.target === event.currentTarget) setOpen(false); }}
      onKeyDown={(event) => {
        if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
        const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('.writer-choice-row'));
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        event.preventDefault();
        buttons[(index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length]?.focus();
      }}>
      <div className="writer-visual-choice-heading">
        <h2>{title}</h2>
        <button type="button" aria-label="Close previews" onClick={() => setOpen(false)}>×</button>
      </div>
      <div className="writer-visual-choice-list">
        {items.map((item, index) => <div key={item.value}>
          {(index === 0 || items[index - 1]?.group !== item.group) &&
            <div className="writer-visual-choice-group">{item.group}</div>}
          <button className="writer-choice-row" type="button"
            aria-pressed={selected === item.value}
            onMouseDown={(event) => event.preventDefault()} onClick={() => choose(item.value)}>
            <span className="writer-choice-preview"
              style={{ fontFamily: '"' + item.font + '", sans-serif', fontSize: item.size,
                fontWeight: item.bold ? 700 : 400 }}>{item.sample}</span>
            <span className="writer-choice-details">
              <span>{item.label}</span>
              {item.note && <small>{item.note}</small>}
            </span>
            <span className="writer-choice-selected" aria-hidden="true">✓</span>
          </button>
        </div>)}
      </div>
    </dialog>}
  </>;
}
