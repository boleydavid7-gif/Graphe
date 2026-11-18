import {
  forwardRef, memo, useCallback, useEffect, useId, useImperativeHandle, useLayoutEffect,
  useRef, useState, type PointerEvent as ReactPointerEvent,
} from 'react';
import { EditorSurface, type EditorSurfaceHandle, type EditorSurfaceProps } from './EditorSurface';
import { SelectionActions } from './SelectionActions';
import { TableGrid } from './TableGrid';
import { ImageLayer } from './ImageLayer';
import type { LayoutOrigin, PreviewZoom } from './PagePreview';
import type {
  CaretGeometry, LayoutAction, LayoutGlyphOutline, LayoutLine, LayoutPage, LayoutSelectionView, LayoutView,
  ImageView, WriterCommand, WriterSelection, WriterView,
} from './types';

export interface PageInfo {
  readonly pageCount: number | null;
  readonly currentPage: number | null;
  readonly status: 'loading' | 'ready' | 'unavailable';
  /** Measured presentation scale only, including fit-width responsiveness. */
  readonly zoomPercent?: number | null;
}

export interface PagedEditorProps extends EditorSurfaceProps {
  /** Read the live Rust snapshot rather than a potentially queued React view. */
  readonly getCurrentView: () => WriterView | null;
  readonly zoom: PreviewZoom;
  readonly loadLayout: (origin: LayoutOrigin) => LayoutView;
  readonly loadGeometry: (origin: LayoutOrigin) => LayoutSelectionView;
  readonly onInteraction: (action: LayoutAction, expectedOrigin?: LayoutOrigin) => WriterView | undefined;
  readonly onPageInfo?: (info: PageInfo) => void;
}

const EMPTY_IMAGE_URLS: ReadonlyMap<string, string> = new Map();

interface Display {
  readonly status: 'loading' | 'ready' | 'error';
  readonly snapshot?: LayoutView;
  readonly selection?: LayoutSelectionView;
  readonly error?: string;
}

interface Drag {
  readonly pointerId: number;
  readonly touch: boolean;
  readonly startX: number;
  readonly startY: number;
  x: number;
  y: number;
  readonly origin: LayoutOrigin;
  readonly endpoint?: 'anchor' | 'focus';
  readonly offsetX?: number;
  readonly offsetY?: number;
  readonly capture?: Element;
  moved?: boolean;
  longPressed?: boolean;
  released?: boolean;
  expectedSelection: WriterSelection | null;
}

const HOLD_DELAY_MS = 550;
const TOUCH_SLOP = 12;

interface RevealRequest {
  readonly origin: LayoutOrigin;
  readonly selection: WriterSelection | null;
}

type HitIntent = { readonly type: 'hit'; readonly extend: boolean }
  | { readonly type: 'select_word' }
  | { readonly type: 'drag_endpoint'; readonly endpoint: 'anchor' | 'focus' };

function origin(view: WriterView): LayoutOrigin {
  return { documentId: view.documentId, sessionId: view.sessionId, revision: view.revision };
}

function sameOrigin(left: LayoutOrigin | undefined, right: LayoutOrigin): boolean {
  return !!left && left.documentId === right.documentId && left.sessionId === right.sessionId
    && left.revision === right.revision;
}

function collapsed(view: WriterView): boolean {
  return !view.selection || (view.selection.anchor.paragraphId === view.selection.focus.paragraphId
    && view.selection.anchor.utf16 === view.selection.focus.utf16);
}

function sameSelection(left: WriterSelection | null, right: WriterSelection | null): boolean {
  return left === right || (!!left && !!right
    && left.anchor.paragraphId === right.anchor.paragraphId && left.anchor.utf16 === right.anchor.utf16
    && left.focus.paragraphId === right.focus.paragraphId && left.focus.utf16 === right.focus.utf16);
}

function message(failure: unknown): string {
  return failure instanceof Error ? failure.message : 'The document could not be laid out. Your work is unchanged.';
}

interface DefinitionProps {
  readonly outlines: readonly LayoutGlyphOutline[]; readonly namespace: string;
}

interface LineProps {
  readonly lines: readonly LayoutLine[]; readonly namespace: string;
}

function sameDefinitions(previous: DefinitionProps, next: DefinitionProps): boolean {
  if (previous.namespace !== next.namespace || previous.outlines.length !== next.outlines.length) return false;
  if (previous.outlines === next.outlines) return true;
  for (let index = 0; index < previous.outlines.length; index += 1) {
    const before = previous.outlines[index];
    const after = next.outlines[index];
    if (!before || !after || before.id !== after.id || before.fontId !== after.fontId
        || before.glyphId !== after.glyphId || before.unitsPerEm !== after.unitsPerEm || before.path !== after.path) return false;
  }
  return true;
}

function sameLines(previous: LineProps, next: LineProps): boolean {
  if (previous.namespace !== next.namespace || previous.lines.length !== next.lines.length) return false;
  if (previous.lines === next.lines) return true;
  for (let index = 0; index < previous.lines.length; index += 1) {
    const before = previous.lines[index];
    const after = next.lines[index];
    if (!before || !after || before.paragraphId !== after.paragraphId || before.lineIndex !== after.lineIndex
        || before.textRange.startUtf16 !== after.textRange.startUtf16 || before.textRange.endUtf16 !== after.textRange.endUtf16
        || before.x !== after.x || before.y !== after.y || before.width !== after.width || before.height !== after.height
        || before.baseline !== after.baseline || before.glyphs.length !== after.glyphs.length
        || before.decorations.length !== after.decorations.length) return false;
    for (let glyphIndex = 0; glyphIndex < before.glyphs.length; glyphIndex += 1) {
      const beforeGlyph = before.glyphs[glyphIndex];
      const afterGlyph = after.glyphs[glyphIndex];
      if (!beforeGlyph || !afterGlyph || beforeGlyph.outlineId !== afterGlyph.outlineId
          || beforeGlyph.transform !== afterGlyph.transform || beforeGlyph.color !== afterGlyph.color) return false;
    }
    for (let decorationIndex = 0; decorationIndex < before.decorations.length; decorationIndex += 1) {
      const beforeDecoration = before.decorations[decorationIndex];
      const afterDecoration = after.decorations[decorationIndex];
      if (!beforeDecoration || !afterDecoration || beforeDecoration.x !== afterDecoration.x
          || beforeDecoration.y !== afterDecoration.y || beforeDecoration.width !== afterDecoration.width
          || beforeDecoration.thickness !== afterDecoration.thickness || beforeDecoration.color !== afterDecoration.color) return false;
    }
  }
  return true;
}

// JSON snapshots have new object references even when a page did not change.
// These comparators cover every rendered property; update them with the markup
// below. They compare Rust output only and never derive text or page geometry.
const GlyphDefinitions = memo(function GlyphDefinitions({ outlines, namespace }: DefinitionProps) {
  return <svg className="layout-glyph-definitions" width="0" height="0" aria-hidden="true" focusable="false">
    <defs>{outlines.map((outline) => <path key={outline.id} id={`${namespace}-editing-glyph-${outline.id}`} d={outline.path}
      data-layout-outline-id={outline.id} />)}</defs>
  </svg>;
}, sameDefinitions);

const GlyphLines = memo(function GlyphLines({ lines, namespace }: LineProps) {
  return <>{lines.map((line) => <g key={JSON.stringify([line.paragraphId, line.lineIndex])}
    data-layout-line={line.lineIndex} data-layout-paragraph-id={line.paragraphId}
    data-layout-utf16-start={line.textRange.startUtf16} data-layout-utf16-end={line.textRange.endUtf16}
    data-layout-x={line.x} data-layout-y={line.y} data-layout-width={line.width}
    data-layout-height={line.height} data-layout-baseline={line.baseline}>
    {line.glyphs.map((glyph, index) => <use key={index} href={`#${namespace}-editing-glyph-${glyph.outlineId}`}
      transform={glyph.transform} fill={glyph.color} data-layout-outline-id={glyph.outlineId} />)}
    {line.decorations.map((decoration, index) => <rect key={index} x={decoration.x} y={decoration.y}
      width={decoration.width} height={decoration.thickness} fill={decoration.color} data-layout-decoration="underline" />)}
  </g>)}</>;
}, sameLines);

/** Input is a controlled semantic projection; every visible page coordinate is Rust output. */
export const PagedEditor = forwardRef<EditorSurfaceHandle, PagedEditorProps>(function PagedEditor(props, ref) {
  const latest = useRef(props);
  const root = useRef<HTMLDivElement>(null);
  const input = useRef<EditorSurfaceHandle>(null);
  const proxy = useRef<HTMLDivElement>(null);
  const pages = useRef(new Map<number, SVGSVGElement>());
  const namespace = useId().replace(/[^A-Za-z0-9_-]/g, '');
  const frame = useRef<number | undefined>(undefined);
  const viewportFrame = useRef<number | undefined>(undefined);
  const completedTouchTap = useRef(false);
  const dragFrame = useRef<number | undefined>(undefined);
  const drag = useRef<Drag | null>(null);
  const holdTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const touchPointers = useRef(new Set<number>());
  const cancelledTouch = useRef(false);
  const selectionHandles = useRef(new Map<'anchor' | 'focus', HTMLButtonElement>());
  const selectionTools = useRef<HTMLDivElement>(null);
  const revealEndpoint = useRef<'anchor' | 'focus'>('focus');
  const composingRef = useRef(false);
  const pendingOrigin = useRef<LayoutOrigin | undefined>(undefined);
  const pendingReveal = useRef<RevealRequest | null>(null);
  const infoSignature = useRef('');
  const fallbackRef = useRef(false);
  const activeDocument = useRef(props.view.documentId);
  const previousFallback = useRef(false);
  const [fallback, setFallback] = useState(false);
  const [focused, setFocused] = useState(false);
  const [composition, setComposition] = useState('');
  const [composing, setComposing] = useState(false);
  const [touchSelection, setTouchSelection] = useState<Pick<LayoutOrigin, 'sessionId' | 'documentId'> | null>(null);
  const [draggingHandle, setDraggingHandle] = useState(false);
  const [compositionPosition, setCompositionPosition] = useState({ left: 0, top: 0 });
  const [display, setDisplay] = useState<Display>({ status: 'loading' });
  const current = useRef(display);

  const cancelGesture = useCallback(() => {
    clearTimeout(holdTimer.current);
    holdTimer.current = undefined;
    const previous = drag.current;
    drag.current = null;
    completedTouchTap.current = false;
    if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    dragFrame.current = undefined;
    if (previous?.capture?.hasPointerCapture(previous.pointerId)) previous.capture.releasePointerCapture(previous.pointerId);
    setDraggingHandle(false);
  }, []);

  const gestureCurrent = useCallback((active: Drag) => {
    const live = latest.current.getCurrentView();
    return !!live && !latest.current.disabled && !composingRef.current
      && sameOrigin(active.origin, origin(live)) && sameSelection(active.expectedSelection, live.selection);
  }, []);

  const gestureCanFocus = useCallback((active: Drag) => {
    const live = latest.current.getCurrentView();
    return !!live && !latest.current.disabled && !composingRef.current
      && live.documentId === active.origin.documentId && live.sessionId === active.origin.sessionId;
  }, []);

  useLayoutEffect(() => {
    latest.current = props;
    if (drag.current && !gestureCurrent(drag.current)
        && !(drag.current.released && gestureCanFocus(drag.current))) cancelGesture();
    const requested = pendingReveal.current;
    if (requested && (!sameOrigin(requested.origin, origin(props.view))
        || !sameSelection(requested.selection, props.view.selection))) pendingReveal.current = null;
  });

  useEffect(() => {
    function down(event: PointerEvent) {
      if (event.pointerType !== 'touch') return;
      if (!touchPointers.current.size) cancelledTouch.current = false;
      touchPointers.current.add(event.pointerId);
      if (touchPointers.current.size > 1) {
        cancelledTouch.current = true;
        cancelGesture();
      }
    }
    function end(event: PointerEvent) {
      if (event.pointerType === 'touch') {
        const active = drag.current;
        if (event.type === 'pointerup' && active?.touch && !active.endpoint && active.pointerId === event.pointerId) {
          // Capture release before target listeners can update the document or
          // focus. A stale completed tap may focus the current input, but must
          // never issue a stale layout hit. Other cancellation remains strict.
          active.released = true;
          clearTimeout(holdTimer.current);
          // Use a task rather than a microtask: trusted browser events may run
          // microtask checkpoints between native capture and bubble listeners.
          holdTimer.current = setTimeout(() => {
            holdTimer.current = undefined;
            if (drag.current === active && active.released) cancelGesture();
          }, 0);
        }
        touchPointers.current.delete(event.pointerId);
        if (!touchPointers.current.size) cancelledTouch.current = false;
      }
    }
    function inactive() {
      cancelGesture();
      touchPointers.current.clear();
    }
    function visibility() { if (document.visibilityState === 'hidden') inactive(); }
    document.addEventListener('pointerdown', down, true);
    document.addEventListener('pointerup', end, true);
    document.addEventListener('pointercancel', end, true);
    document.addEventListener('visibilitychange', visibility);
    window.addEventListener('blur', inactive);
    const pointers = touchPointers.current;
    return () => {
      document.removeEventListener('pointerdown', down, true);
      document.removeEventListener('pointerup', end, true);
      document.removeEventListener('pointercancel', end, true);
      document.removeEventListener('visibilitychange', visibility);
      window.removeEventListener('blur', inactive);
      clearTimeout(holdTimer.current);
      pointers.clear();
      if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    };
  }, [cancelGesture]);
  useLayoutEffect(() => {
    if (previousFallback.current !== fallback) {
      previousFallback.current = fallback;
      input.current?.focus();
    }
  }, [fallback]);

  const publish = useCallback((value: Display) => {
    current.current = value;
    setDisplay(value);
  }, []);

  const scrollContainer = useCallback((): HTMLElement | null => root.current?.closest<HTMLElement>('.workspace') ?? null, []);
  const bounds = useCallback(() => {
    const scroll = scrollContainer();
    const rect = scroll?.getBoundingClientRect() ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight);
    const visual = window.visualViewport;
    const left = visual?.offsetLeft ?? 0;
    const top = visual?.offsetTop ?? 0;
    const right = left + (visual?.width ?? window.innerWidth);
    const bottom = top + (visual?.height ?? window.innerHeight);
    const x = Math.max(rect.left, left);
    const y = Math.max(rect.top, top);
    const width = Math.min(rect.right, right) - x;
    const height = Math.min(rect.bottom, bottom) - y;
    return width > 0 && height > 0 ? new DOMRect(x, y, width, height)
      : new DOMRect(left, top, right - left, bottom - top);
  }, [scrollContainer]);

  const reportInfo = useCallback((status: PageInfo['status'], snapshot?: LayoutView) => {
    const viewport = bounds();
    let currentPage: number | null = null;
    let visibleHeight = -1;
    if (status === 'ready' && snapshot) {
      for (const page of snapshot.layout.pages) {
        const rect = pages.current.get(page.number)?.getBoundingClientRect();
        if (!rect) continue;
        const height = Math.max(0, Math.min(rect.bottom, viewport.bottom) - Math.max(rect.top, viewport.top));
        if (height > visibleHeight) { visibleHeight = height; currentPage = page.number; }
      }
      currentPage ??= snapshot.layout.pages[0]?.number ?? null;
    }
    const reference = snapshot?.layout.pages[0];
    const referenceBounds = reference ? pages.current.get(reference.number)?.getBoundingClientRect() : undefined;
    const zoomPercent = status === 'ready' && snapshot && reference && referenceBounds
      ? Math.round(referenceBounds.width / (reference.width / snapshot.layout.unitsPerPoint * (96 / 72)) * 100) : null;
    const info: PageInfo = { status, pageCount: status === 'ready' && snapshot ? snapshot.layout.pages.length : null,
      currentPage, zoomPercent };
    const signature = JSON.stringify(info);
    if (signature !== infoSignature.current) {
      infoSignature.current = signature;
      latest.current.onPageInfo?.(info);
    }
  }, [bounds]);

  const screenCaret = useCallback((snapshot: LayoutView, caret: Pick<CaretGeometry, 'pageNumber' | 'rect'>): DOMRect | null => {
    const page = snapshot.layout.pages.find((candidate) => candidate.number === caret.pageNumber);
    const surface = pages.current.get(caret.pageNumber);
    if (!page || !surface) return null;
    const rect = surface.getBoundingClientRect();
    if (!rect.width || !rect.height) return null;
    return new DOMRect(rect.left + caret.rect.x / page.width * rect.width,
      rect.top + caret.rect.y / page.height * rect.height,
      caret.rect.width / page.width * rect.width, caret.rect.height / page.height * rect.height);
  }, []);

  const positionSelectionUi = useCallback((snapshot: LayoutView, geometry: LayoutSelectionView['geometry'], viewport: DOMRect) => {
    const scale = Math.max(1, window.visualViewport?.scale ?? 1);
    const visible = (rect: DOMRect | null) => rect && rect.bottom > viewport.top && rect.top < viewport.bottom
      && rect.left >= viewport.left && rect.left < viewport.right;
    for (const endpoint of ['anchor', 'focus'] as const) {
      const handle = selectionHandles.current.get(endpoint);
      if (!handle) continue;
      const rect = geometry ? screenCaret(snapshot, geometry[endpoint]) : null;
      handle.style.visibility = drag.current?.endpoint === endpoint || visible(rect) ? 'visible' : 'hidden';
      if (rect) {
        const height = 44 / scale;
        const above = endpoint === 'anchor' && rect.top - height >= viewport.top
          || endpoint === 'focus' && rect.bottom + height > viewport.bottom;
        handle.style.left = `${rect.left}px`;
        handle.style.top = `${Math.max(viewport.top, Math.min(above ? rect.top - height : rect.bottom, viewport.bottom - height))}px`;
        handle.dataset.placement = above ? 'above' : 'below';
        handle.style.setProperty('--selection-ui-scale', `${1 / scale}`);
      }
    }
    const tools = selectionTools.current;
    if (tools && geometry) {
      tools.style.setProperty('--selection-ui-scale', `${1 / scale}`);
      tools.style.maxWidth = `${Math.max(0, viewport.width * scale - 16)}px`;
      const size = tools.getBoundingClientRect();
      const focus = screenCaret(snapshot, geometry.focus);
      const anchor = screenCaret(snapshot, geometry.anchor);
      const target = visible(focus) ? focus : visible(anchor) ? anchor : null;
      const x = target?.left ?? (viewport.left + viewport.right) / 2;
      const left = Math.max(viewport.left + 4 / scale, Math.min(viewport.right - size.width - 4 / scale, x - size.width / 2));
      const handles = [...selectionHandles.current.values()].map((handle) => handle.getBoundingClientRect()).filter(visible);
      const first = Math.min(target?.top ?? viewport.top, ...handles.map((rect) => rect.top));
      const last = Math.max(target?.bottom ?? viewport.top, ...handles.map((rect) => rect.bottom));
      const candidates = [first - size.height - 8 / scale, last + 8 / scale,
        (target?.top ?? viewport.top) - size.height - 8 / scale, viewport.top + 4 / scale,
        viewport.bottom - size.height - 4 / scale].map((top) =>
        Math.max(viewport.top + 4 / scale, Math.min(viewport.bottom - size.height - 4 / scale, top)));
      // Keep the action buttons out of the handles' touch targets, including
      // when the first selected line sits just below the application chrome.
      const top = candidates.find((candidate) => handles.every((rect) =>
        left + size.width <= rect.left || left >= rect.right || candidate + size.height <= rect.top || candidate >= rect.bottom))
        ?? candidates[0]!;
      tools.style.left = `${left}px`;
      tools.style.top = `${top}px`;
      tools.style.visibility = 'visible';
    }
  }, [screenCaret]);

  const positionInput = useCallback((reveal = false) => {
    const scroll = scrollContainer();
    if (scroll) {
      const visual = window.visualViewport;
      // Once the browser magnifies the viewport, inner scroll boundaries must
      // allow panning to reach that viewport. Retain normal containment at 1x.
      if ((visual?.scale ?? 1) > 1) scroll.style.setProperty('--writer-viewport-overscroll', 'auto');
      else scroll.style.removeProperty('--writer-viewport-overscroll');
      // The inherited inset pads the inner page content, not this flex item.
      // It adds scroll room without inflating the workspace on keyboard/rotation.
      // Both Android Chrome and iOS Safari can resize the visual viewport
      // independently of the layout viewport. Reserve keyboard room only
      // while the native editing proxy is focused: browser chrome collapsing
      // must not create a permanent empty strip under the document.
      const editing = !!input.current && (
        root.current?.contains(document.activeElement) ?? false
      );
      const inset = editing && visual ? Math.min(scroll.clientHeight,
        Math.max(0, scroll.getBoundingClientRect().bottom - visual.offsetTop - visual.height)) : 0;
      const value = `${inset}px`;
      if (scroll.style.getPropertyValue('--writer-keyboard-inset') !== value) {
        scroll.style.setProperty('--writer-keyboard-inset', value);
      }
    }
    const value = current.current;
    const selection = value.selection?.geometry;
    const snapshot = value.snapshot;
    if (!snapshot || fallbackRef.current) return;
    reportInfo('ready', snapshot);
    if (!selection || !proxy.current) return;
    const caret = screenCaret(snapshot, selection.focus);
    if (!caret) return;
    const viewport = bounds();
    const revealCaret = screenCaret(snapshot, selection[revealEndpoint.current]) ?? caret;
    const requested = pendingReveal.current;
    const explicitReveal = !!requested && sameOrigin(requested.origin, snapshot)
      && sameOrigin(value.selection, requested.origin)
      && sameOrigin(requested.origin, origin(latest.current.view))
      && sameSelection(requested.selection, latest.current.view.selection)
      && requested.selection?.anchor.paragraphId === selection.anchor.position.paragraphId
      && requested.selection.anchor.utf16 === selection.anchor.position.utf16
      && requested.selection.focus.paragraphId === selection.focus.position.paragraphId
      && requested.selection.focus.utf16 === selection.focus.position.utf16;
    if (explicitReveal) pendingReveal.current = null;
    if (!drag.current?.endpoint && (explicitReveal || (reveal && document.activeElement?.getAttribute('data-testid') === 'editor'))) {
      const target = explicitReveal ? caret : revealCaret;
      const margin = Math.min(28, viewport.height / 4);
      const minimalTop = target.top < viewport.top + margin ? target.top - viewport.top - margin
        : target.bottom > viewport.bottom - margin ? target.bottom - viewport.bottom + margin : 0;
      // An explicit Find result should be comfortably visible on its page,
      // rather than just peeking past the viewport edge. Ordinary typing keeps
      // the smaller reveal delta so it does not recenter the page on each key.
      const top = explicitReveal && minimalTop !== 0
        ? (target.top + target.bottom - viewport.top - viewport.bottom) / 2 : minimalTop;
      const left = target.left < viewport.left + margin ? target.left - viewport.left - margin
        : target.right > viewport.right - margin ? target.right - viewport.right + margin : 0;
      if (top || left) {
        const scroll = scrollContainer();
        if (scroll) scroll.scrollBy({ top, left, behavior: 'instant' });
        else window.scrollBy({ top, left, behavior: 'instant' });
      }
    }
    const resolved = screenCaret(snapshot, selection.focus) ?? caret;
    positionSelectionUi(snapshot, selection, bounds());
    proxy.current.style.left = `${Math.max(viewport.left, Math.min(viewport.right - 2, resolved.left))}px`;
    proxy.current.style.top = `${Math.max(viewport.top, Math.min(viewport.bottom - 20, resolved.top))}px`;
    // A constant-size native input avoids a large font's candidate window
    // being pushed down by invisible proxy content. The visible caret keeps
    // its complete Rust-measured height independently.
    proxy.current.style.height = '20px';
    if (composingRef.current) {
      setCompositionPosition((previous) => previous.left === resolved.left && previous.top === resolved.top
        ? previous : { left: resolved.left, top: resolved.top });
    }
    if (reveal && proxy.current.contains(document.activeElement)) input.current?.restoreSelection();
    reportInfo('ready', snapshot);
  }, [bounds, positionSelectionUi, reportInfo, screenCaret, scrollContainer]);

  const refresh = useCallback((requested?: LayoutOrigin) => {
    pendingOrigin.current = requested ?? origin(latest.current.view);
    if (frame.current !== undefined) cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      frame.current = undefined;
      const expected = pendingOrigin.current ?? origin(latest.current.view);
      const changedDocument = activeDocument.current !== expected.documentId;
      if (changedDocument) {
        activeDocument.current = expected.documentId;
        fallbackRef.current = false;
        setFallback(false);
      }
      if (fallbackRef.current) { reportInfo('unavailable'); return; }
      try {
        const cached = current.current.snapshot;
        const snapshot = sameOrigin(cached, expected) ? cached! : latest.current.loadLayout(expected);
        const selection = latest.current.loadGeometry(expected);
        if (!sameOrigin(snapshot, expected) || !sameOrigin(selection, expected)
            || snapshot.layout.documentId !== expected.documentId) {
          throw new Error('The document changed while updating its pages. Try again.');
        }
        publish({ status: 'ready', snapshot, selection });
      } catch (failure) {
        publish({ status: 'error', error: message(failure) });
        reportInfo('unavailable');
      }
    });
  }, [publish, reportInfo]);

  const refreshSelection = useCallback((view: WriterView) => {
    input.current?.syncView(view);
    // A visual gesture changes Rust selection without changing its revision.
    // Synchronize an already focused native input before the next key. Setting
    // a DOM range inside a blurred contenteditable can itself focus it; a hold
    // timer must leave that to trusted pointerup or the explicit Edit command.
    if (root.current?.contains(document.activeElement)
        && document.activeElement?.getAttribute('data-testid') === 'editor') input.current?.restoreSelection();
    const expected = origin(view);
    if (sameOrigin(current.current.snapshot, expected)) {
      try {
        const selection = latest.current.loadGeometry(expected);
        if (!sameOrigin(selection, expected)) throw new Error('The document changed while updating its selection.');
        publish({ ...current.current, selection });
      } catch (failure) { latest.current.onError(message(failure)); }
    } else refresh(expected);
  }, [publish, refresh]);

  const interact = useCallback((action: LayoutAction, expectedOrigin?: LayoutOrigin): WriterView | undefined => {
    if (latest.current.disabled) return undefined;
    const next = latest.current.onInteraction(action, expectedOrigin);
    if (next) refreshSelection(next);
    return next;
  }, [refreshSelection]);

  useEffect(() => {
    refresh(origin(props.view));
    return () => { if (frame.current !== undefined) cancelAnimationFrame(frame.current); };
  }, [props.view, refresh]);

  useLayoutEffect(() => {
    if (display.status === 'ready') {
      // This is a display transform of Rust rectangles, not browser text layout.
      const queued = requestAnimationFrame(() => positionInput(true));
      return () => cancelAnimationFrame(queued);
    }
  }, [display, props.zoom, props.disabled, touchSelection, draggingHandle, positionInput]);

  useEffect(() => {
    let reveal = false;
    const updateViewport = () => {
      if (drag.current?.touch && !drag.current.endpoint && !drag.current.longPressed && !drag.current.released) cancelGesture();
      if (viewportFrame.current !== undefined) return;
      viewportFrame.current = requestAnimationFrame(() => {
        viewportFrame.current = undefined;
        const shouldReveal = reveal;
        reveal = false;
        positionInput(shouldReveal);
      });
    };
    const visualChanged = () => {
      // Keep the Rust caret visible as the software keyboard animates on
      // either mobile platform; the RAF coalesces resize/scroll bursts.
      reveal = true;
      if (drag.current?.touch && !drag.current.released) cancelGesture();
      updateViewport();
    };
    const focusChanged = () => {
      // Focus can change without a viewport event (hardware keyboard,
      // Android keyboard dismissal, or switching to a toolbar control).
      updateViewport();
    };
    document.addEventListener('focusin', focusChanged);
    document.addEventListener('focusout', focusChanged);
    window.addEventListener('scroll', updateViewport, true);
    window.addEventListener('resize', updateViewport);
    window.visualViewport?.addEventListener('resize', visualChanged);
    window.visualViewport?.addEventListener('scroll', visualChanged);
    const scroll = scrollContainer();
    const observed = scroll ?? root.current;
    const resizeObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(updateViewport);
    if (observed) resizeObserver?.observe(observed);
    return () => {
      document.removeEventListener('focusin', focusChanged);
      document.removeEventListener('focusout', focusChanged);
      window.removeEventListener('scroll', updateViewport, true);
      window.removeEventListener('resize', updateViewport);
      window.visualViewport?.removeEventListener('resize', visualChanged);
      window.visualViewport?.removeEventListener('scroll', visualChanged);
      resizeObserver?.disconnect();
      scroll?.style.removeProperty('--writer-keyboard-inset');
      scroll?.style.removeProperty('--writer-viewport-overscroll');
      if (viewportFrame.current !== undefined) cancelAnimationFrame(viewportFrame.current);
      if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    };
  }, [cancelGesture, positionInput, scrollContainer]);

  useImperativeHandle(ref, () => ({
    focus: () => { input.current?.focus(); refresh(); },
    restoreSelection: () => { input.current?.restoreSelection(); refresh(); },
    syncView: (view) => { input.current?.syncView(view); refresh(origin(view)); },
    focusDocument: () => root.current?.focus({ preventScroll: true }),
    revealSelection: (view) => {
      revealEndpoint.current = 'focus';
      if (fallbackRef.current || !view.selection) { pendingReveal.current = null; return; }
      pendingReveal.current = { origin: origin(view), selection: view.selection };
      refreshSelection(view);
    },
  }), [refresh, refreshSelection]);

  const hit = useCallback((clientX: number, clientY: number, intent: HitIntent): WriterView | undefined => {
    const value = current.current;
    if (fallbackRef.current || value.status !== 'ready' || !value.snapshot
        || !sameOrigin(value.snapshot, origin(latest.current.view))
        || !sameOrigin(value.snapshot, pendingOrigin.current ?? origin(latest.current.view))) return undefined;
    let target: { page: LayoutPage; bounds: DOMRect; distance: number } | undefined;
    for (const page of value.snapshot.layout.pages) {
      const rect = pages.current.get(page.number)?.getBoundingClientRect();
      if (!rect) continue;
      // Choose the displayed page only. Text hit-testing inside it is Rust's job.
      const dx = Math.max(rect.left - clientX, 0, clientX - rect.right);
      const dy = Math.max(rect.top - clientY, 0, clientY - rect.bottom);
      const distance = dx * dx + dy * dy;
      if (!target || distance < target.distance) target = { page, bounds: rect, distance };
    }
    if (!target?.bounds.width || !target.bounds.height) return undefined;
    return interact({ ...intent, pageNumber: target.page.number,
      x: Math.round((clientX - target.bounds.left) / target.bounds.width * target.page.width),
      y: Math.round((clientY - target.bounds.top) / target.bounds.height * target.page.height) }, value.snapshot);
  }, [interact]);

  const autoScroll = useCallback(function tick() {
    const active = drag.current;
    if (!active || (active.touch && !active.endpoint)) return;
    if (!gestureCurrent(active)) { cancelGesture(); return; }
    const viewport = bounds();
    const zoom = Math.max(1, window.visualViewport?.scale ?? 1);
    const edge = 28 / zoom;
    const top = active.y < viewport.top + edge ? -18 / zoom : active.y > viewport.bottom - edge ? 18 / zoom : 0;
    const left = active.x < viewport.left + edge ? -18 / zoom : active.x > viewport.right - edge ? 18 / zoom : 0;
    if (top || left) {
      const scroll = scrollContainer();
      if (scroll) scroll.scrollBy({ top, left });
      else window.scrollBy({ top, left });
      const next = hit(active.x + (active.offsetX ?? 0), active.y + (active.offsetY ?? 0), active.endpoint
        ? { type: 'drag_endpoint', endpoint: active.endpoint } : { type: 'hit', extend: true });
      if (next) active.expectedSelection = next.selection;
    }
    dragFrame.current = requestAnimationFrame(tick);
  }, [bounds, cancelGesture, gestureCurrent, hit, scrollContainer]);

  function pointerDown(event: ReactPointerEvent<SVGSVGElement>): void {
    completedTouchTap.current = false;
    if (event.button !== 0 || props.disabled || composingRef.current || cancelledTouch.current) return;
    cancelGesture();
    const live = latest.current.getCurrentView();
    if (!live) return;
    revealEndpoint.current = 'focus';
    if (!props.view.paragraphs.length && event.pointerType !== 'touch') {
      // Empty native files have no paragraph identity/caret until the first
      // Rust edit materializes content. Focusing must not synthesize one in JS.
      event.preventDefault();
      input.current?.focus();
      return;
    }
    const active: Drag = { pointerId: event.pointerId, touch: event.pointerType === 'touch',
      startX: event.clientX, startY: event.clientY, x: event.clientX, y: event.clientY,
      origin: origin(live), expectedSelection: live.selection,
      capture: event.pointerType === 'touch' ? undefined : event.currentTarget };
    drag.current = active;
    if (active.touch) {
      holdTimer.current = setTimeout(() => {
        holdTimer.current = undefined;
        if (drag.current !== active || active.moved || cancelledTouch.current || !gestureCurrent(active)) return;
        const next = hit(active.x, active.y, { type: 'select_word' });
        if (next) {
          active.expectedSelection = next.selection;
          active.longPressed = true;
          setTouchSelection({ sessionId: next.sessionId, documentId: next.documentId });
          // A timer is not a trusted keyboard activation. Focus, if requested,
          // completes on the user's pointerup or the explicit Edit control.
        }
      }, HOLD_DELAY_MS);
      return;
    }
    setTouchSelection(null);
    event.preventDefault();
    const next = hit(event.clientX, event.clientY, { type: 'hit', extend: event.shiftKey });
    if (!next) { drag.current = null; return; }
    active.expectedSelection = next.selection;
    event.currentTarget.setPointerCapture(event.pointerId);
    input.current?.focus();
    if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    dragFrame.current = requestAnimationFrame(autoScroll);
  }

  function pointerMove(event: ReactPointerEvent<SVGSVGElement>): void {
    const active = drag.current;
    if (!active || active.pointerId !== event.pointerId) return;
    active.x = event.clientX;
    active.y = event.clientY;
    if (!gestureCurrent(active)) { cancelGesture(); return; }
    if (active.touch && Math.hypot(active.x - active.startX, active.y - active.startY) >= TOUCH_SLOP) {
      active.moved = true;
      clearTimeout(holdTimer.current);
      holdTimer.current = undefined;
    }
    if (!active.touch) {
      event.preventDefault();
      const next = hit(active.x, active.y, { type: 'hit', extend: true });
      if (next) active.expectedSelection = next.selection;
    }
  }

  function pointerUp(event: ReactPointerEvent<SVGSVGElement>): void {
    const active = drag.current;
    if (!active || active.pointerId !== event.pointerId) return;
    const fresh = gestureCurrent(active);
    const tap = active.touch && !active.moved && !cancelledTouch.current && gestureCanFocus(active)
      && Math.hypot(event.clientX - active.startX, event.clientY - active.startY) < TOUCH_SLOP;
    // Complete the gesture before focusing. Synchronous focus/viewport events
    // must not mistake this finished tap for a pending hold and cancel its
    // compatibility-click recovery.
    clearTimeout(holdTimer.current);
    holdTimer.current = undefined;
    drag.current = null;
    if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    dragFrame.current = undefined;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (tap) {
      completedTouchTap.current = true;
      if (!active.longPressed) {
        if (fresh && latest.current.view.paragraphs.length) hit(event.clientX, event.clientY, { type: 'hit', extend: event.shiftKey });
        setTouchSelection(null);
      }
      // A stale layout must not swallow a tap or invent a new position. Focus
      // synchronously and retain the latest Rust selection when hit testing fails.
      const live = latest.current.getCurrentView();
      if (live) input.current?.syncView(live);
      input.current?.focus();
    }
  }

  function pointerCancel(event: ReactPointerEvent<Element>): void {
    completedTouchTap.current = false;
    if (drag.current?.pointerId !== event.pointerId) return;
    cancelGesture();
  }

  function handleDown(event: ReactPointerEvent<HTMLButtonElement>, endpoint: 'anchor' | 'focus') {
    if (event.button !== 0 || props.disabled || composingRef.current || cancelledTouch.current) return;
    const live = latest.current.getCurrentView();
    const value = current.current;
    if (!live || !value.snapshot || !value.selection?.geometry || !sameOrigin(value.snapshot, origin(live))) return;
    if (!sameSelection(live.selection, { anchor: value.selection.geometry.anchor.position,
      focus: value.selection.geometry.focus.position })) return;
    // At small display scales, accessible hit areas can overlap. Resolve the
    // nearest physical handle without reordering its logical Rust endpoint.
    let nearest = Number.POSITIVE_INFINITY;
    let chosen = endpoint;
    for (const candidate of [endpoint, endpoint === 'anchor' ? 'focus' : 'anchor'] as const) {
      const element = selectionHandles.current.get(candidate);
      if (!element || getComputedStyle(element).visibility === 'hidden') continue;
      const rect = element.getBoundingClientRect();
      const distance = Math.hypot(event.clientX - rect.left - rect.width / 2, event.clientY - rect.top - rect.height / 2);
      if (distance < nearest) { nearest = distance; chosen = candidate; }
    }
    endpoint = chosen;
    const caret = screenCaret(value.snapshot, value.selection.geometry[endpoint]);
    if (!caret) return;
    cancelGesture();
    event.preventDefault();
    event.stopPropagation();
    revealEndpoint.current = endpoint;
    drag.current = { pointerId: event.pointerId, touch: event.pointerType === 'touch', endpoint,
      startX: event.clientX, startY: event.clientY, x: event.clientX, y: event.clientY,
      offsetX: caret.left - event.clientX, offsetY: caret.top + caret.height / 2 - event.clientY,
      origin: origin(live), expectedSelection: live.selection, capture: event.currentTarget };
    event.currentTarget.setPointerCapture(event.pointerId);
    setDraggingHandle(true);
    dragFrame.current = requestAnimationFrame(autoScroll);
  }

  function handleMove(event: ReactPointerEvent<HTMLButtonElement>) {
    const active = drag.current;
    if (!active?.endpoint || active.pointerId !== event.pointerId) return;
    if (!gestureCurrent(active)) { cancelGesture(); return; }
    event.preventDefault();
    active.x = event.clientX;
    active.y = event.clientY;
    const next = hit(active.x + (active.offsetX ?? 0), active.y + (active.offsetY ?? 0),
      { type: 'drag_endpoint', endpoint: active.endpoint });
    if (next) active.expectedSelection = next.selection;
  }

  function handleUp(event: ReactPointerEvent<HTMLButtonElement>) {
    if (drag.current?.pointerId !== event.pointerId) return;
    handleMove(event);
    cancelGesture();
  }


  function commitImageEdit(imageId: string, action: WriterCommand): void {
    const live = latest.current.getCurrentView();
    const displayed = current.current;
    if (!live || latest.current.disabled || composingRef.current
        || displayed.status !== 'ready' || !displayed.snapshot
        || live.selectedImageId !== imageId
        || !sameOrigin(displayed.snapshot, origin(live))) return;
    const next = latest.current.onCommand({ ...action, expectedRevision: live.revision });
    if (next) {
      input.current?.syncView(next);
      refresh(origin(next));
    }
  }

  // Find a structural insertion point in Rust's already-positioned body flow.
  // Images remain in-flow, not floating overlays: dropping before a paragraph,
  // table or image moves the image block rather than inventing a CSS position.
  function commitImageMove(imageId: string, alignment: ImageView['alignment'],
    clientX: number, clientY: number): void {
    const live = latest.current.getCurrentView();
    const value = current.current;
    if (!live || !value.snapshot || !sameOrigin(value.snapshot, origin(live))) return;
    const blocks = live.bodyBlocks ?? [];
    const source = blocks.find((block) => block.id === imageId && block.kind === 'image');
    if (!source || live.selectedImageId !== imageId) return;
    let nearest: { page: LayoutPage; bounds: DOMRect; distance: number } | undefined;
    for (const page of value.snapshot.layout.pages) {
      if (page.sectionId !== source.sectionId) continue;
      const bounds = pages.current.get(page.number)?.getBoundingClientRect();
      if (!bounds || !bounds.height || !bounds.width) continue;
      const dx = Math.max(bounds.left - clientX, 0, clientX - bounds.right);
      const dy = Math.max(bounds.top - clientY, 0, clientY - bounds.bottom);
      const distance = dx * dx + dy * dy;
      if (!nearest || distance < nearest.distance) nearest = { page, bounds, distance };
    }
    if (!nearest) return;
    const hoverPage = nearest.page.number;
    const hoverY = (clientY - nearest.bounds.top) / nearest.bounds.height * nearest.page.height;
    const sectionBlocks = blocks.filter((block) => block.sectionId === source.sectionId);
    const candidates = sectionBlocks.filter((block) => block.id !== imageId);
    const locate = (block: (typeof candidates)[number]): { pageNumber: number; y: number } | undefined => {
      for (const page of value.snapshot!.layout.pages) {
        if (page.sectionId !== source.sectionId) continue;
        const y = block.kind === 'paragraph'
          ? page.lines.find((line) => line.paragraphId === block.id)?.y
          : block.kind === 'table'
            ? page.tableCells?.find((cell) => cell.tableId === block.id)?.rect.y
            : page.images?.find((image) => image.imageId === block.id)?.rect.y;
        if (y !== undefined) return { pageNumber: page.number, y };
      }
      return undefined;
    };
    const beforeBlockId = candidates.find((block) => {
      const found = locate(block);
      return !!found && (found.pageNumber > hoverPage || found.pageNumber === hoverPage && found.y > hoverY);
    })?.id ?? null;
    const originalIndex = sectionBlocks.findIndex((block) => block.id === imageId);
    const originalNext = sectionBlocks[originalIndex + 1]?.id ?? null;
    if (originalNext === beforeBlockId && live.images?.find((image) => image.id === imageId)?.alignment === alignment) return;
    commitImageEdit(imageId, { type: 'move_image', beforeBlockId, alignment });
  }

  const snapshot = display.snapshot;
  const layout = snapshot?.documentId === props.view.documentId && snapshot.sessionId === props.view.sessionId
    ? snapshot.layout : undefined;
  const ready = display.status === 'ready' && sameOrigin(snapshot, origin(props.view));
  const selection = ready ? display.selection?.geometry : undefined;
  const showTouchSelection = ready && !!selection && !props.disabled && !composing
    && touchSelection?.documentId === props.view.documentId && touchSelection.sessionId === props.view.sessionId
    && (!collapsed(props.view) || draggingHandle);
  const scale = props.zoom === 'fit' ? 1 : Number(props.zoom) / 100;
  const widest = layout?.pages.reduce((maximum, page) => Math.max(maximum, page.width), 0) ?? 0;
  const empty = !(props.view.images?.length) && props.view.paragraphs.length <= 1 && props.view.paragraphs.every((paragraph) =>
    paragraph.runs.every((run) => run.text === ''));

  function enterTextView(): void {
    pendingReveal.current = null;
    fallbackRef.current = true;
    setFallback(true);
    reportInfo('unavailable');
  }

  function inputSurface(inputOnly: boolean) {
    return <EditorSurface key={inputOnly ? 'paged-input' : 'text-input'} ref={input}
      view={props.view} disabled={props.disabled} inputOnly={inputOnly} imageUrls={props.imageUrls}
      onCommand={(command) => {
        revealEndpoint.current = 'focus';
        const next = latest.current.onCommand(command);
        if (next && inputOnly) refresh(origin(next));
        return next;
      }} onError={props.onError} onCopy={props.onCopy}
      onNavigate={inputOnly ? (action) => { revealEndpoint.current = 'focus'; return interact(action); } : undefined}
      onPageScroll={(direction) => {
        const scroll = scrollContainer();
        const height = scroll?.clientHeight ?? window.innerHeight;
        if (scroll) scroll.scrollBy({ top: (direction === 'up' ? -1 : 1) * height * 0.8 });
        else window.scrollBy({ top: (direction === 'up' ? -1 : 1) * height * 0.8 });
      }}
      onCompositionText={setComposition}
      onCompositionChange={(active) => {
        composingRef.current = active;
        if (active) { cancelGesture(); setTouchSelection(null); revealEndpoint.current = 'focus'; }
        setComposing(active);
        props.onCompositionChange?.(active);
        if (active) positionInput();
        if (!active) refresh();
      }} />;
  }

  return <div ref={root} className={`paged-editor${fallback ? ' text-mode' : ''}`} data-testid="paged-editor"
    role="group" aria-label={props.view.selectedImageId
      ? `Selected image: ${props.view.images?.find((image) => image.id === props.view.selectedImageId)?.altText || 'Document image'}. Enter to continue writing; Delete to remove.`
      : 'Document editor'} tabIndex={-1} data-selected-image-id={props.view.selectedImageId ?? ''}
    data-layout-document-id={snapshot?.documentId} data-layout-revision={snapshot?.revision}
    aria-busy={!fallback && display.status !== 'error' && !ready}
    onFocusCapture={(event) => { if (event.target instanceof HTMLElement && event.target.dataset.testid === 'editor') setFocused(true); }}
    onBlurCapture={(event) => { if (!(event.relatedTarget instanceof Node) || !root.current?.contains(event.relatedTarget)) setFocused(false); }}
    onKeyDown={(event) => {
      if (event.target !== root.current || props.disabled || composingRef.current || event.nativeEvent.isComposing) return;
      const modifier = event.ctrlKey || event.metaKey;
      let command: Parameters<EditorSurfaceProps['onCommand']>[0] | undefined;
      if (modifier && !event.altKey && event.key.toLowerCase() === 'z') command = { type: event.shiftKey ? 'redo' : 'undo' };
      else if (modifier && !event.altKey && event.key.toLowerCase() === 'y') command = { type: 'redo' };
      else if (props.view.selectedImageId && !modifier && !event.altKey) {
        if (event.key === 'Delete' || event.key === 'Backspace') command = { type: 'delete_image' };
        else if (event.key === 'Enter' || event.key === 'Escape' || event.key.startsWith('Arrow')) command = {
          type: 'move_image_cursor', direction: event.key === 'ArrowLeft' || event.key === 'ArrowUp' ? 'backward' : 'forward',
        };
      }
      if (command) {
        event.preventDefault();
        const next = props.onCommand(command);
        if (next) {
          input.current?.syncView(next);
          refresh(origin(next));
          if (command.type === 'move_image_cursor') input.current?.focus();
        }
      }
    }}
    onCopyCapture={(event) => { if (props.view.selectedImageId) { event.preventDefault(); event.stopPropagation(); props.onError('Image clipboard copying is not supported yet. Your image is unchanged.'); } }}
    onCutCapture={(event) => { if (props.view.selectedImageId) { event.preventDefault(); event.stopPropagation(); props.onError('Image clipboard cutting is not supported yet. Use Delete image and Undo.'); } }}
    onPasteCapture={(event) => { if (props.view.selectedImageId) { event.preventDefault(); event.stopPropagation(); props.onError('Press Enter or choose Back to text before pasting. Your image is unchanged.'); } }}>
    {fallback ? <>
      <div className="paged-editor-notice" role="status"><span>Text view · Page layout unavailable. Your document is preserved.</span>
        <button onClick={() => { fallbackRef.current = false; setFallback(false); refresh(); }}>Try pages again</button></div>
      {!!props.view.images?.length && <div className="fallback-image-controls" role="group" aria-label="Document images">
        {props.view.images.map((image) => <button key={image.id} disabled={props.disabled || composing}
          aria-pressed={props.view.selectedImageId === image.id} onClick={() => {
            if (composingRef.current || latest.current.disabled) return;
            const next = props.onCommand({ type: 'select_image', imageId: image.id, expectedRevision: props.view.revision });
            if (next) { input.current?.syncView(next); root.current?.focus({ preventScroll: true }); }
          }}>Select image: {image.altText || 'Document image'}</button>)}
      </div>}
      {inputSurface(false)}
    </> : <>
      <div ref={proxy} className="paged-editor-input" style={{ position: 'fixed', left: 0, top: 0, width: 2,
        height: 20, overflow: 'hidden', opacity: 0, pointerEvents: 'none' }}>
        {inputSurface(true)}
      </div>
      {display.status === 'error' ? <div className="paged-editor-error" role="alert">
        <h2>Page layout is unavailable</h2><p>{display.error}</p>
        <p>Your text and formatting are intact. You can continue writing in text view.</p>
        <button onClick={enterTextView}>Continue in text view</button>
        <button onClick={() => refresh()}>Try pages again</button>
      </div> : <>
        {!ready && !layout && <p className="paged-editor-loading" role="status">Updating pages…</p>}
        {layout && <>
          {layout.diagnostics.some((diagnostic) => diagnostic.kind === 'missing_glyph')
            && <div className="paged-editor-notice" role="status">
              <span>Some characters are unavailable in the page fonts. Text view can use your browser's fonts.</span>
              <button onClick={enterTextView}>Use text view</button>
            </div>}
          {layout.diagnostics.length > 0 && <details className="paged-editor-notices"><summary>Layout notices ({layout.diagnostics.length})</summary>
            <ul>{layout.diagnostics.map((diagnostic, index) => <li key={index} data-layout-diagnostic-kind={diagnostic.kind}>{diagnostic.message}</li>)}</ul>
          </details>}
          <GlyphDefinitions outlines={layout.outlines} namespace={namespace} />
          <div className="paged-editor-pages" style={{ width: widest / layout.unitsPerPoint * (96 / 72) * scale,
            maxWidth: props.zoom === 'fit' ? '100%' : 'none', marginInline: 'auto' }}>
            {layout.pages.map((page) => <figure key={page.number} className="paged-editor-page" data-editor-page={page.number}
              style={{ width: `${page.width / widest * 100}%` }}>
              <div className="paged-editor-paper" style={{ aspectRatio: `${page.width} / ${page.height}` }}>
                <svg ref={(element) => { if (element) pages.current.set(page.number, element); else pages.current.delete(page.number); }}
                  viewBox={`0 0 ${page.width} ${page.height}`} width="100%" height="100%" aria-hidden="true" focusable="false"
                  data-layout-page={page.number} data-layout-width={page.width} data-layout-height={page.height}
                  onPointerDown={pointerDown} onPointerMove={pointerMove} onPointerUp={pointerUp} onPointerCancel={pointerCancel}
                  onLostPointerCapture={(event) => { if (drag.current?.pointerId === event.pointerId) cancelGesture(); }}
                  onContextMenu={(event) => { if (drag.current?.touch || showTouchSelection) event.preventDefault(); }}
                  onClick={(event) => {
                    if (!completedTouchTap.current || latest.current.disabled || composingRef.current) return;
                    completedTouchTap.current = false;
                    event.preventDefault();
                    // Compatibility mouse events may have blurred the proxy
                    // after pointerup. Complete focus in this same tap gesture.
                    input.current?.focus();
                  }}
                  // Keep native magnification available alongside document
                  // scrolling; pan-y alone disables the browser's pinch zoom.
                  style={{ touchAction: 'pan-x pan-y pinch-zoom', userSelect: 'none', cursor: 'text' }}>
                  <TableGrid page={page} />
                  <ImageLayer page={page} images={props.view.images ?? []} urls={props.imageUrls ?? EMPTY_IMAGE_URLS}
                    selectedId={props.view.selectedImageId} unitsPerPoint={layout.unitsPerPoint}
                    disabled={props.disabled}
                    onResize={(id, widthPt, heightPt, lockAspect) => commitImageEdit(id,
                      { type: 'resize_image', widthPt, ...(heightPt !== undefined ? { heightPt } : {}), lockAspect })}
                    onMove={commitImageMove} onSelect={(id) => {
                      if (props.disabled || composingRef.current || !snapshot || !sameOrigin(snapshot, origin(props.getCurrentView() ?? props.view))) return;
                      cancelGesture();
                      setTouchSelection(null);
                      const next = props.onCommand({ type: 'select_image', imageId: id, expectedRevision: snapshot.revision });
                      if (next) { input.current?.syncView(next); root.current?.focus({ preventScroll: true }); refresh(origin(next)); }
                    }} />
                  {!props.view.selectedImageId && selection?.rectangles.filter((rectangle) => rectangle.pageNumber === page.number).map((rectangle, index) =>
                    <rect key={`selection-${index}`} className="paged-editor-selection" {...rectangle.rect} data-editor-selection="true" />)}
                  <GlyphLines lines={page.lines} namespace={namespace} />
                  {!props.view.selectedImageId && selection?.focus.pageNumber === page.number && focused && collapsed(props.view) && !composing
                    && <rect className="paged-editor-caret" {...selection.focus.rect} data-editor-caret="true"
                      data-editor-paragraph-id={selection.focus.position.paragraphId}
                      data-editor-utf16={selection.focus.position.utf16} data-editor-grapheme={selection.focus.position.grapheme} />}
                </svg>
                {empty && selection?.focus.pageNumber === page.number && !composing
                  && <span className="paged-editor-placeholder" aria-hidden="true" data-editor-placeholder="true"
                    style={{ position: 'absolute', pointerEvents: 'none',
                      left: `${selection.focus.rect.x / page.width * 100}%`,
                      top: `${selection.focus.rect.y / page.height * 100}%`,
                      height: `${selection.focus.rect.height / page.height * 100}%`,
                      display: 'flex', alignItems: 'center', marginLeft: 6 }}>Start writing…</span>}
              </div><figcaption>Page {page.number}</figcaption>
            </figure>)}
          </div>
        </>}
      </>}
      {showTouchSelection && <>
        {(['anchor', 'focus'] as const).map((endpoint) => <button type="button" key={endpoint}
          ref={(element) => { if (element) selectionHandles.current.set(endpoint, element); else selectionHandles.current.delete(endpoint); }}
          className="touch-selection-handle" data-testid={`touch-selection-handle-${endpoint}`}
          data-paragraph-id={selection[endpoint].position.paragraphId} data-utf16={selection[endpoint].position.utf16}
          data-page-number={selection[endpoint].pageNumber} data-affinity={selection[endpoint].position.affinity}
          aria-hidden="true" tabIndex={-1} style={{ left: 0, top: 0, visibility: 'hidden' }}
          onPointerDown={(event) => handleDown(event, endpoint)} onPointerMove={handleMove} onPointerUp={handleUp}
          onPointerCancel={pointerCancel}
          onLostPointerCapture={(event) => { if (drag.current?.pointerId === event.pointerId) cancelGesture(); }}
          onContextMenu={(event) => event.preventDefault()} />)}
        {!draggingHandle && <div ref={selectionTools} className="touch-selection-tools"
          style={{ left: 0, top: 0, visibility: 'hidden' }}>
          <SelectionActions view={props.view} disabled={props.disabled} composing={composing}
            getCurrentView={props.getCurrentView} canInteract={() => !latest.current.disabled && !composingRef.current}
            onCopy={props.onCopy} onError={props.onError}
            onCommand={(command) => {
              revealEndpoint.current = 'focus';
              const next = latest.current.onCommand(command);
              if (next) { input.current?.syncView(next); refresh(origin(next)); }
              return next;
            }}
            onFocusEditor={() => { revealEndpoint.current = 'focus'; input.current?.focus(); positionInput(true); }} />
        </div>}
      </>}
      {composing && composition && <span className="paged-editor-composition" style={{ position: 'fixed',
        left: compositionPosition.left, top: compositionPosition.top, pointerEvents: 'none' }} aria-hidden="true">{composition}</span>}
    </>}
  </div>;
});
