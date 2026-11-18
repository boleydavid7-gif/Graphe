import { useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import type { ImageView, LayoutPage, LayoutRect } from './types';

type Alignment = ImageView['alignment'];
type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';
type Point = { x: number; y: number };
interface Gesture {
  readonly id: string;
  readonly start: LayoutRect;
  readonly image: ImageView;
  readonly initial: Point;
  readonly pointers: Map<number, Point>;
  readonly handle: Handle | null;
  readonly kind: 'move' | 'resize' | 'pinch';
  rect: LayoutRect;
  distance?: number;
  pinchBase?: LayoutRect;
  changed: boolean;
}
const handles: readonly Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];
const between = (n: number, min: number, max: number) => Math.max(min, Math.min(max, n));
const distance = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);

function alignmentFor(rect: LayoutRect, page: LayoutPage): Alignment {
  const { x, width } = page.contentRect;
  const choices: readonly { alignment: Alignment; x: number }[] = [
    { alignment: 'left', x },
    { alignment: 'center', x: x + (width - rect.width) / 2 },
    { alignment: 'right', x: x + width - rect.width },
  ];
  return choices.reduce((best, candidate) =>
    Math.abs(candidate.x - rect.x) < Math.abs(best.x - rect.x) ? candidate : best).alignment;
}

/** Rust supplies all persisted placement. Pointer previews never change document data. */
export function ImageLayer({ page, images, urls, selectedId, onSelect, onResize, onMove, unitsPerPoint = 64, disabled = false }: {
  readonly page: LayoutPage;
  readonly images: readonly ImageView[];
  readonly urls: ReadonlyMap<string, string>;
  readonly selectedId?: string | null;
  readonly onSelect?: (id: string) => void;
  readonly onResize?: (id: string, widthPt: number, heightPt: number | undefined, lockAspect: boolean) => void;
  readonly onMove?: (id: string, alignment: Alignment, clientX: number, clientY: number) => void;
  readonly unitsPerPoint?: number;
  readonly disabled?: boolean;
}) {
  const active = useRef<Gesture | null>(null);
  const [preview, setPreview] = useState<{ id: string; rect: LayoutRect } | null>(null);
  const [dragging, setDragging] = useState(false);
  const editable = !disabled && !!onSelect && !!onResize && !!onMove;
  const coarse = typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches;
  const handleSize = Math.max(8, Math.min(10, page.width / unitsPerPoint / 72)) * unitsPerPoint;
  const hitSize = (coarse ? Math.max(28, Math.min(46, page.width / unitsPerPoint / 14))
    : Math.max(12, Math.min(17, page.width / unitsPerPoint / 44))) * unitsPerPoint;
  const activeHandles = coarse ? handles.filter((handle) => handle.length === 2) : handles;
  const lineWidth = Math.max(1, page.width / unitsPerPoint / 650) * unitsPerPoint;

  function point(event: ReactPointerEvent<SVGGElement>): Point | null {
    const svg = event.currentTarget.ownerSVGElement;
    if (!svg) return null;
    const bounds = svg.getBoundingClientRect();
    if (!bounds.width || !bounds.height) return null;
    return {
      x: (event.clientX - bounds.left) * page.width / bounds.width,
      y: (event.clientY - bounds.top) * page.height / bounds.height,
    };
  }

  function start(event: ReactPointerEvent<SVGGElement>, image: ImageView, rect: LayoutRect): void {
    if (!editable || event.button !== 0) return;
    const here = point(event);
    if (!here) return;
    event.preventDefault();
    event.stopPropagation();
    const existing = active.current;
    if (existing?.id === image.id && event.pointerType === 'touch' && existing.pointers.size === 1) {
      existing.pointers.set(event.pointerId, here);
      const [first, second] = [...existing.pointers.values()];
      if (!first || !second) return;
      existing.distance = Math.max(1, distance(first, second));
      existing.pinchBase = existing.rect;
      active.current = { ...existing, kind: 'pinch', changed: existing.changed };
      event.currentTarget.setPointerCapture(event.pointerId);
      return;
    }
    if (existing) return;
    const handle = (event.target as Element).closest?.('[data-image-resize-handle]')
      ?.getAttribute('data-image-resize-handle') as Handle | null;
    if (selectedId !== image.id) onSelect?.(image.id);
    active.current = {
      id: image.id, start: rect, image, initial: here, pointers: new Map([[event.pointerId, here]]),
      kind: handle && selectedId === image.id ? 'resize' : 'move',
      handle: handle && selectedId === image.id ? handle : null,
      rect, changed: false,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function move(event: ReactPointerEvent<SVGGElement>): void {
    const gesture = active.current;
    if (!gesture?.pointers.has(event.pointerId)) return;
    const here = point(event);
    if (!here) return;
    event.preventDefault();
    event.stopPropagation();
    gesture.pointers.set(event.pointerId, here);
    const { start: original, initial, handle } = gesture;
    let next: LayoutRect = original;
    const minLength = unitsPerPoint * 8;
    const maxWidth = page.contentRect.width;
    const maxHeight = page.contentRect.height;
    if (gesture.kind === 'pinch' && gesture.pointers.size === 2) {
      const [a, b] = [...gesture.pointers.values()];
      if (!a || !b) return;
      const base = gesture.pinchBase ?? original;
      const scale = distance(a, b) / (gesture.distance ?? 1);
      const width = between(base.width * scale, minLength,
        Math.min(maxWidth, maxHeight * original.width / original.height));
      next = { ...base, width, height: width * original.height / original.width };
    } else if (gesture.kind === 'resize' && handle) {
      const deltaX = here.x - initial.x;
      const deltaY = here.y - initial.y;
      if (handle.length === 2) {
        const factorX = (handle.includes('w') ? -deltaX : deltaX) / original.width;
        const factorY = (handle.includes('n') ? -deltaY : deltaY) / original.height;
        const factor = Math.abs(factorX) > Math.abs(factorY) ? factorX : factorY;
        const width = between(original.width * (1 + factor), minLength,
          Math.min(maxWidth, maxHeight * original.width / original.height));
        next = { ...original, width, height: width * original.height / original.width };
      } else if (handle === 'w' || handle === 'e') {
        const width = between(original.width + (handle === 'w' ? -deltaX : deltaX), minLength, maxWidth);
        next = { ...original, width };
      } else {
        const height = between(original.height + (handle === 'n' ? -deltaY : deltaY), minLength, maxHeight);
        next = { ...original, height };
      }
    } else {
      next = { ...original,
        x: between(original.x + here.x - initial.x, page.contentRect.x,
          page.contentRect.x + page.contentRect.width - original.width),
        y: original.y + here.y - initial.y,
      };
    }
    const changed = Math.abs(next.x - original.x) + Math.abs(next.y - original.y)
      + Math.abs(next.width - original.width) + Math.abs(next.height - original.height) > 3 * unitsPerPoint;
    gesture.rect = next;
    gesture.changed ||= changed;
    if (gesture.changed) { setPreview({ id: gesture.id, rect: next }); setDragging(true); }
  }

  function finish(event: ReactPointerEvent<SVGGElement>, commit: boolean): void {
    const gesture = active.current;
    if (!gesture?.pointers.has(event.pointerId)) return;
    event.preventDefault();
    event.stopPropagation();
    const rect = gesture.rect;
    const changed = gesture.changed;
    const kind = gesture.kind;
    for (const pointerId of gesture.pointers.keys()) {
      if (event.currentTarget.hasPointerCapture(pointerId)) event.currentTarget.releasePointerCapture(pointerId);
    }
    active.current = null;
    setPreview(null);
    setDragging(false);
    if (!commit || !changed) return;
    if (kind === 'move') {
      onMove?.(gesture.id, alignmentFor(rect, page), event.clientX, event.clientY);
    } else {
      const widthPt = Math.round(rect.width / unitsPerPoint * 20) / 20;
      const heightPt = Math.round(rect.height / unitsPerPoint * 20) / 20;
      const lockAspect = kind === 'pinch' || !!gesture.handle && gesture.handle.length === 2;
      onResize?.(gesture.id, widthPt, lockAspect ? undefined : heightPt, lockAspect);
    }
  }

  return <g className="document-images">{page.images?.map((placement) => {
    const image = images.find((item) => item.id === placement.imageId);
    if (!image) return null;
    const selected = selectedId === image.id;
    const url = urls.get(image.id);
    const rect = preview?.id === image.id ? preview.rect : placement.rect;
    const canEdit = editable && selected;
    return <g key={image.id} className={dragging && preview?.id === image.id ? 'document-image-dragging' : undefined}
      style={{ cursor: editable ? canEdit ? 'move' : 'pointer' : undefined,
        touchAction: editable ? 'none' : 'pan-x pan-y pinch-zoom' }}
      onPointerDown={editable ? (event) => start(event, image, placement.rect) : undefined}
      onPointerMove={editable ? move : undefined}
      onPointerUp={editable ? (event) => finish(event, true) : undefined}
      onPointerCancel={editable ? (event) => finish(event, false) : undefined}
      onLostPointerCapture={editable ? (event) => {
        if (active.current?.pointers.has(event.pointerId)) {
          active.current = null;
          setPreview(null);
          setDragging(false);
        }
      } : undefined}
      onClick={editable ? (event) => event.stopPropagation() : undefined}>
      <title>{image.altText || 'Document image'}</title>
      {url ? <image {...rect} href={url} preserveAspectRatio="none" style={{ pointerEvents: editable ? 'all' : 'none' }}
        data-document-image-id={image.id} />
        : <rect {...rect} fill="#eee9e1" data-image-loading={image.id} />}
      {canEdit && <>
        <rect {...rect} fill="none" stroke="var(--writer-accent, #b78e37)" strokeWidth={lineWidth}
          pointerEvents="none" data-image-selected="true" />
        {activeHandles.map((handle) => {
          const x = rect.x + (handle.includes('w') ? 0 : handle.includes('e') ? rect.width : rect.width / 2);
          const y = rect.y + (handle.includes('n') ? 0 : handle.includes('s') ? rect.height : rect.height / 2);
          return <g key={handle}>
            <rect x={x - hitSize / 2} y={y - hitSize / 2}
              width={hitSize} height={hitSize} rx={hitSize / 5}
              fill="transparent" stroke="none" data-image-resize-handle={handle}
              style={{ touchAction: 'none',
                cursor: handle === 'n' || handle === 's' ? 'ns-resize'
                  : handle === 'w' || handle === 'e' ? 'ew-resize'
                    : handle === 'nw' || handle === 'se' ? 'nwse-resize' : 'nesw-resize' }} />
            <rect x={x - handleSize / 2} y={y - handleSize / 2} width={handleSize}
              height={handleSize} rx={handleSize / 5}
              fill="#fff" stroke="var(--writer-accent, #b78e37)" strokeWidth={lineWidth}
              pointerEvents="none" />
          </g>;
        })}
      </>}
    </g>;
  })}</g>;
}
