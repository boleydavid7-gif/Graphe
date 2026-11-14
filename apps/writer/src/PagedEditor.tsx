import {
  forwardRef, memo, useCallback, useEffect, useId, useImperativeHandle, useLayoutEffect,
  useRef, useState, type PointerEvent as ReactPointerEvent,
} from 'react';
import { EditorSurface, type EditorSurfaceHandle, type EditorSurfaceProps } from './EditorSurface';
import type { LayoutOrigin, PreviewZoom } from './PagePreview';
import type {
  CaretGeometry, LayoutAction, LayoutGlyphOutline, LayoutLine, LayoutPage, LayoutSelectionView, LayoutView,
  WriterSelection, WriterView,
} from './types';

export interface PageInfo {
  readonly pageCount: number | null;
  readonly currentPage: number | null;
  readonly status: 'loading' | 'ready' | 'unavailable';
  /** Measured presentation scale only, including fit-width responsiveness. */
  readonly zoomPercent?: number | null;
}

export interface PagedEditorProps extends EditorSurfaceProps {
  readonly zoom: PreviewZoom;
  readonly loadLayout: (origin: LayoutOrigin) => LayoutView;
  readonly loadGeometry: (origin: LayoutOrigin) => LayoutSelectionView;
  readonly onInteraction: (action: LayoutAction, expectedOrigin?: LayoutOrigin) => WriterView | undefined;
  readonly onPageInfo?: (info: PageInfo) => void;
}

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
}

interface RevealRequest {
  readonly origin: LayoutOrigin;
  readonly selection: WriterSelection | null;
}

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
  const dragFrame = useRef<number | undefined>(undefined);
  const drag = useRef<Drag | null>(null);
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
  const [compositionPosition, setCompositionPosition] = useState({ left: 0, top: 0 });
  const [display, setDisplay] = useState<Display>({ status: 'loading' });
  const current = useRef(display);

  useLayoutEffect(() => {
    latest.current = props;
    const requested = pendingReveal.current;
    if (requested && (!sameOrigin(requested.origin, origin(props.view))
        || !sameSelection(requested.selection, props.view.selection))) pendingReveal.current = null;
  });
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
    return scroll ? scroll.getBoundingClientRect() : new DOMRect(0, 0, window.innerWidth, window.innerHeight);
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

  const screenCaret = useCallback((snapshot: LayoutView, caret: CaretGeometry): DOMRect | null => {
    const page = snapshot.layout.pages.find((candidate) => candidate.number === caret.pageNumber);
    const surface = pages.current.get(caret.pageNumber);
    if (!page || !surface) return null;
    const rect = surface.getBoundingClientRect();
    if (!rect.width || !rect.height) return null;
    return new DOMRect(rect.left + caret.rect.x / page.width * rect.width,
      rect.top + caret.rect.y / page.height * rect.height,
      caret.rect.width / page.width * rect.width, caret.rect.height / page.height * rect.height);
  }, []);

  const positionInput = useCallback((reveal = false) => {
    const value = current.current;
    const selection = value.selection?.geometry;
    const snapshot = value.snapshot;
    if (!snapshot || fallbackRef.current) return;
    reportInfo('ready', snapshot);
    if (!selection || !proxy.current) return;
    const caret = screenCaret(snapshot, selection.focus);
    if (!caret) return;
    const viewport = bounds();
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
    if (explicitReveal || (reveal && document.activeElement?.getAttribute('data-testid') === 'editor')) {
      const margin = Math.min(28, viewport.height / 4);
      const minimalTop = caret.top < viewport.top + margin ? caret.top - viewport.top - margin
        : caret.bottom > viewport.bottom - margin ? caret.bottom - viewport.bottom + margin : 0;
      // An explicit Find result should be comfortably visible on its page,
      // rather than just peeking past the viewport edge. Ordinary typing keeps
      // the smaller reveal delta so it does not recenter the page on each key.
      const top = explicitReveal && minimalTop !== 0
        ? (caret.top + caret.bottom - viewport.top - viewport.bottom) / 2 : minimalTop;
      const left = caret.left < viewport.left + margin ? caret.left - viewport.left - margin
        : caret.right > viewport.right - margin ? caret.right - viewport.right + margin : 0;
      if (top || left) {
        const scroll = scrollContainer();
        if (scroll) scroll.scrollBy({ top, left, behavior: 'instant' });
        else window.scrollBy({ top, left, behavior: 'instant' });
      }
    }
    const resolved = screenCaret(snapshot, selection.focus) ?? caret;
    proxy.current.style.left = `${Math.max(0, Math.min(window.innerWidth - 2, resolved.left))}px`;
    proxy.current.style.top = `${Math.max(0, Math.min(window.innerHeight - 20, resolved.top))}px`;
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
  }, [bounds, reportInfo, screenCaret, scrollContainer]);

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
  }, [display, props.zoom, props.disabled, positionInput]);

  useEffect(() => {
    const updateViewport = () => {
      if (viewportFrame.current !== undefined) return;
      viewportFrame.current = requestAnimationFrame(() => {
        viewportFrame.current = undefined;
        positionInput();
      });
    };
    window.addEventListener('scroll', updateViewport, true);
    window.addEventListener('resize', updateViewport);
    window.visualViewport?.addEventListener('resize', updateViewport);
    const observed = scrollContainer() ?? root.current;
    const resizeObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(updateViewport);
    if (observed) resizeObserver?.observe(observed);
    return () => {
      window.removeEventListener('scroll', updateViewport, true);
      window.removeEventListener('resize', updateViewport);
      window.visualViewport?.removeEventListener('resize', updateViewport);
      resizeObserver?.disconnect();
      if (viewportFrame.current !== undefined) cancelAnimationFrame(viewportFrame.current);
      if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    };
  }, [positionInput, scrollContainer]);

  useImperativeHandle(ref, () => ({
    focus: () => { input.current?.focus(); refresh(); },
    restoreSelection: () => { input.current?.restoreSelection(); refresh(); },
    syncView: (view) => { input.current?.syncView(view); refresh(origin(view)); },
    revealSelection: (view) => {
      if (fallbackRef.current || !view.selection) { pendingReveal.current = null; return; }
      pendingReveal.current = { origin: origin(view), selection: view.selection };
      refreshSelection(view);
    },
  }), [refresh, refreshSelection]);

  const hit = useCallback((clientX: number, clientY: number, extend: boolean): WriterView | undefined => {
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
    return interact({ type: 'hit', pageNumber: target.page.number, extend,
      x: Math.round((clientX - target.bounds.left) / target.bounds.width * target.page.width),
      y: Math.round((clientY - target.bounds.top) / target.bounds.height * target.page.height) }, value.snapshot);
  }, [interact]);

  const autoScroll = useCallback(function tick() {
    const active = drag.current;
    if (!active || active.touch) return;
    const viewport = bounds();
    const delta = active.y < viewport.top + 28 ? -18 : active.y > viewport.bottom - 28 ? 18 : 0;
    if (delta) {
      const scroll = scrollContainer();
      if (scroll) scroll.scrollBy({ top: delta });
      else window.scrollBy({ top: delta });
      hit(active.x, active.y, true);
    }
    dragFrame.current = requestAnimationFrame(tick);
  }, [bounds, hit, scrollContainer]);

  function pointerDown(event: ReactPointerEvent<SVGSVGElement>): void {
    if (event.button !== 0 || props.disabled || composing) return;
    if (!props.view.paragraphs.length && event.pointerType !== 'touch') {
      // Empty native files have no paragraph identity/caret until the first
      // Rust edit materializes content. Focusing must not synthesize one in JS.
      event.preventDefault();
      input.current?.focus();
      return;
    }
    drag.current = { pointerId: event.pointerId, touch: event.pointerType === 'touch',
      startX: event.clientX, startY: event.clientY, x: event.clientX, y: event.clientY };
    if (event.pointerType === 'touch') return;
    event.preventDefault();
    const next = hit(event.clientX, event.clientY, event.shiftKey);
    if (!next) { drag.current = null; return; }
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
    if (!active.touch) { event.preventDefault(); hit(active.x, active.y, true); }
  }

  function pointerUp(event: ReactPointerEvent<SVGSVGElement>): void {
    const active = drag.current;
    if (!active || active.pointerId !== event.pointerId) return;
    if (active.touch && Math.hypot(event.clientX - active.startX, event.clientY - active.startY) < 12) {
      if (!latest.current.view.paragraphs.length) input.current?.focus();
      else {
        const next = hit(event.clientX, event.clientY, event.shiftKey);
        if (next) input.current?.focus();
      }
    }
    drag.current = null;
    if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    dragFrame.current = undefined;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }

  function pointerCancel(event: ReactPointerEvent<SVGSVGElement>): void {
    if (drag.current?.pointerId !== event.pointerId) return;
    drag.current = null;
    if (dragFrame.current !== undefined) cancelAnimationFrame(dragFrame.current);
    dragFrame.current = undefined;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }

  const snapshot = display.snapshot;
  const layout = snapshot?.documentId === props.view.documentId && snapshot.sessionId === props.view.sessionId
    ? snapshot.layout : undefined;
  const ready = display.status === 'ready' && sameOrigin(snapshot, origin(props.view));
  const selection = ready ? display.selection?.geometry : undefined;
  const scale = props.zoom === 'fit' ? 1 : Number(props.zoom) / 100;
  const widest = layout?.pages.reduce((maximum, page) => Math.max(maximum, page.width), 0) ?? 0;
  const empty = props.view.paragraphs.length <= 1 && props.view.paragraphs.every((paragraph) =>
    paragraph.runs.every((run) => run.text === ''));

  function enterTextView(): void {
    pendingReveal.current = null;
    fallbackRef.current = true;
    setFallback(true);
    reportInfo('unavailable');
  }

  function inputSurface(inputOnly: boolean) {
    return <EditorSurface key={inputOnly ? 'paged-input' : 'text-input'} ref={input}
      view={props.view} disabled={props.disabled} inputOnly={inputOnly}
      onCommand={(command) => {
        const next = latest.current.onCommand(command);
        if (next && inputOnly) refresh(origin(next));
        return next;
      }} onError={props.onError} onCopy={props.onCopy}
      onNavigate={inputOnly ? interact : undefined}
      onPageScroll={(direction) => {
        const scroll = scrollContainer();
        const height = scroll?.clientHeight ?? window.innerHeight;
        if (scroll) scroll.scrollBy({ top: (direction === 'up' ? -1 : 1) * height * 0.8 });
        else window.scrollBy({ top: (direction === 'up' ? -1 : 1) * height * 0.8 });
      }}
      onCompositionText={setComposition}
      onCompositionChange={(active) => {
        composingRef.current = active;
        setComposing(active);
        props.onCompositionChange?.(active);
        if (active) positionInput();
        if (!active) refresh();
      }} />;
  }

  return <div ref={root} className={`paged-editor${fallback ? ' text-mode' : ''}`} data-testid="paged-editor"
    data-layout-document-id={snapshot?.documentId} data-layout-revision={snapshot?.revision}
    aria-busy={!fallback && display.status !== 'error' && !ready}
    onFocusCapture={(event) => { if (event.target instanceof HTMLElement && event.target.dataset.testid === 'editor') setFocused(true); }}
    onBlurCapture={(event) => { if (!(event.relatedTarget instanceof Node) || !root.current?.contains(event.relatedTarget)) setFocused(false); }}>
    {fallback ? <>
      <div className="paged-editor-notice" role="status"><span>Text view · Page layout unavailable. Your document is preserved.</span>
        <button onClick={() => { fallbackRef.current = false; setFallback(false); refresh(); }}>Try pages again</button></div>
      {inputSurface(false)}
    </> : <>
      <div ref={proxy} className="paged-editor-input" style={{ position: 'fixed', left: 0, top: 0, width: 2,
        height: 20, overflow: 'hidden', clipPath: 'inset(50%)', pointerEvents: 'none' }}>
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
                  style={{ touchAction: 'pan-y', userSelect: 'none', cursor: 'text' }}>
                  {selection?.rectangles.filter((rectangle) => rectangle.pageNumber === page.number).map((rectangle, index) =>
                    <rect key={`selection-${index}`} className="paged-editor-selection" {...rectangle.rect} data-editor-selection="true" />)}
                  <GlyphLines lines={page.lines} namespace={namespace} />
                  {selection?.focus.pageNumber === page.number && focused && collapsed(props.view) && !composing
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
      {composing && composition && <span className="paged-editor-composition" style={{ position: 'fixed',
        left: compositionPosition.left, top: compositionPosition.top, pointerEvents: 'none' }} aria-hidden="true">{composition}</span>}
    </>}
  </div>;
});
