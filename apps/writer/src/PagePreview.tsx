import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import type { PageInfo } from './PagedEditor';
import type { LayoutView } from './types';
import { TableGrid } from './TableGrid';

export interface LayoutOrigin {
  readonly sessionId: string;
  readonly revision: string;
  readonly documentId: string;
}

export type PreviewZoom = 'fit' | '50' | '75' | '100' | '125' | '150' | '200';

interface PagePreviewProps extends LayoutOrigin {
  readonly loadLayout: (origin: LayoutOrigin) => LayoutView;
  readonly zoom: PreviewZoom;
  readonly onZoomChange: (zoom: PreviewZoom) => void;
  readonly onClose: () => void;
  readonly onPageInfo?: (info: PageInfo) => void;
}

type PreviewState =
  | { readonly status: 'loading' }
  | { readonly status: 'ready'; readonly snapshot: LayoutView }
  | { readonly status: 'error'; readonly message: string };

const zoomOptions: readonly { readonly value: PreviewZoom; readonly label: string }[] = [
  { value: 'fit', label: 'Fit width' },
  { value: '50', label: '50%' },
  { value: '75', label: '75%' },
  { value: '100', label: '100%' },
  { value: '125', label: '125%' },
  { value: '150', label: '150%' },
  { value: '200', label: '200%' },
];

/** Read-only projection of Rust page geometry and glyph outlines. */
export function PagePreview({
  sessionId, revision, documentId, loadLayout, zoom, onZoomChange, onClose, onPageInfo,
}: PagePreviewProps) {
  const namespace = useId();
  const instructions = useId();
  const [state, setState] = useState<PreviewState>({ status: 'loading' });
  const [attempt, setAttempt] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const pages = useRef(new Map<number, SVGSVGElement>());
  const latestPageInfo = useRef(onPageInfo);
  const reportedInfo = useRef('');

  useLayoutEffect(() => { latestPageInfo.current = onPageInfo; });

  useEffect(() => {
    let active = true;
    let secondFrame = 0;
    // Paint the loading state before entering synchronous WASM. A new revision
    // mounts a fresh preview, so obsolete geometry is never shown while waiting.
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        if (!active) return;
        try {
          const snapshot = loadLayout({ sessionId, revision, documentId });
          if (snapshot.sessionId !== sessionId || snapshot.revision !== revision
            || snapshot.documentId !== documentId || snapshot.layout.documentId !== documentId) {
            throw new Error('The document changed while preparing page preview. Return to editing and try again.');
          }
          if (active) setState({ status: 'ready', snapshot });
        } catch (failure) {
          if (active) setState({ status: 'error', message: failure instanceof Error
            ? failure.message : 'Page preview could not be prepared. Your document is unchanged.' });
        }
      });
    });
    return () => {
      active = false;
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, [attempt, documentId, loadLayout, revision, sessionId]);

  const snapshot = state.status === 'ready' ? state.snapshot : undefined;
  const layout = snapshot?.layout;
  const outlineId = (id: number) => `${namespace}-glyph-${id}`;
  const scale = zoom === 'fit' ? 1 : Number(zoom) / 100;

  const reportPageInfo = useCallback(() => {
    const status = state.status === 'error' ? 'unavailable' : state.status;
    const viewport = root.current?.closest<HTMLElement>('.workspace')?.getBoundingClientRect()
      ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight);
    let currentPage: number | null = null;
    let zoomPercent: number | null = null;
    let greatestVisibleHeight = -1;
    if (status === 'ready' && layout) {
      for (const page of layout.pages) {
        const rectangle = pages.current.get(page.number)?.getBoundingClientRect();
        if (!rectangle || rectangle.width <= 0 || rectangle.height <= 0) continue;
        const visibleHeight = Math.max(0, Math.min(rectangle.bottom, viewport.bottom)
          - Math.max(rectangle.top, viewport.top));
        if (visibleHeight > greatestVisibleHeight) {
          greatestVisibleHeight = visibleHeight;
          currentPage = page.number;
          // Only the physical SVG display scale is measured here. Rust owns all
          // glyph, paragraph and page geometry, including the native page width.
          zoomPercent = Math.round(rectangle.width / (page.width / layout.unitsPerPoint * (96 / 72)) * 100);
        }
      }
      currentPage ??= layout.pages[0]?.number ?? null;
    }
    const info: PageInfo = { status, currentPage, zoomPercent,
      pageCount: status === 'ready' && layout ? layout.pages.length : null };
    const signature = JSON.stringify(info);
    if (signature !== reportedInfo.current) {
      reportedInfo.current = signature;
      latestPageInfo.current?.(info);
    }
  }, [layout, state.status]);

  useEffect(() => {
    let frame: number | undefined;
    const update = () => {
      if (frame !== undefined) return;
      frame = requestAnimationFrame(() => { frame = undefined; reportPageInfo(); });
    };
    const observer = new ResizeObserver(update);
    if (root.current) observer.observe(root.current);
    for (const page of pages.current.values()) observer.observe(page);
    update();
    window.addEventListener('scroll', update, true);
    window.addEventListener('resize', update);
    window.visualViewport?.addEventListener('resize', update);
    return () => {
      observer.disconnect();
      if (frame !== undefined) cancelAnimationFrame(frame);
      window.removeEventListener('scroll', update, true);
      window.removeEventListener('resize', update);
      window.visualViewport?.removeEventListener('resize', update);
    };
  }, [reportPageInfo, zoom]);

  return <div ref={root} id="writer-page-preview" className="layout-preview"
    role="region" aria-label="Document page preview" aria-describedby={instructions}
    aria-busy={state.status === 'loading'} tabIndex={-1}
    data-layout-document-id={snapshot?.documentId}
    data-layout-revision={snapshot?.revision}
    data-layout-session-id={snapshot?.sessionId}
    data-layout-font-catalog-id={layout?.fontCatalogId}>
    <div className="layout-preview-controls">
      <div><h1>Page preview</h1><p id={instructions}>Read-only page preview. Return to editing to change text.</p></div>
      <label>Zoom <select aria-label="Preview zoom" value={zoom} onChange={(event) => {
        const option = zoomOptions.find((candidate) => candidate.value === event.target.value);
        if (option) onZoomChange(option.value);
      }}>{zoomOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
      <button onClick={onClose}>Return to editing</button>
    </div>
    {state.status === 'loading' && <p className="layout-preview-message" role="status">Preparing page preview…</p>}
    {state.status === 'error' && <div className="layout-preview-error" role="alert">
      <p>{state.message}</p><button onClick={() => { setState({ status: 'loading' }); setAttempt((value) => value + 1); }}>Try preview again</button>
    </div>}
    {layout && <>
      <p className="layout-page-count" role="status">{layout.pages.length} {layout.pages.length === 1 ? 'page' : 'pages'}</p>
      {layout.diagnostics.length > 0 && <details className="layout-notices">
        <summary>{layout.diagnostics.length} layout {layout.diagnostics.length === 1 ? 'notice' : 'notices'}</summary>
        <ul>{layout.diagnostics.map((diagnostic, index) => <li key={index}
          data-layout-diagnostic-kind={diagnostic.kind}>{diagnostic.message}</li>)}</ul>
      </details>}
      <div className="visually-hidden" role="document" aria-label="Page preview text">
        {layout.pages.map((page) => <section key={page.number} aria-label={`Page ${page.number}`}>
          <h2>Page {page.number}</h2>{page.lines.map((line) =>
            <p key={JSON.stringify([line.paragraphId, line.lineIndex])} dir="auto">{line.text}</p>)}
        </section>)}
      </div>
      <svg className="layout-glyph-definitions" width="0" height="0" aria-hidden="true" focusable="false">
        <defs>{layout.outlines.map((outline) => <path key={outline.id} id={outlineId(outline.id)}
          d={outline.path} data-layout-outline-id={outline.id} />)}</defs>
      </svg>
      {layout.pages.length === 0 ? <p className="layout-preview-message">This document is empty.</p>
        : <div className="layout-preview-scroll"><div className={`layout-preview-pages${zoom === 'fit' ? '' : ' actual-size'}`}>
          {layout.pages.map((page) => <figure key={page.number} className="layout-page-figure"
            data-layout-page={page.number} data-layout-width={page.width} data-layout-height={page.height}
            style={{ width: page.width / layout.unitsPerPoint * (96 / 72) * scale, maxWidth: zoom === 'fit' ? '100%' : 'none' }}>
            <div className="layout-page-paper" style={{ aspectRatio: `${page.width} / ${page.height}` }}>
              <svg ref={(element) => { if (element) pages.current.set(page.number, element); else pages.current.delete(page.number); }}
                viewBox={`0 0 ${page.width} ${page.height}`} width="100%" height="100%" aria-hidden="true" focusable="false">
                <TableGrid page={page} />
                {page.lines.map((line) => <g key={JSON.stringify([line.paragraphId, line.lineIndex])}
                  data-layout-line={line.lineIndex} data-layout-paragraph-id={line.paragraphId}
                  data-layout-utf16-start={line.textRange.startUtf16} data-layout-utf16-end={line.textRange.endUtf16}
                  data-layout-x={line.x} data-layout-y={line.y} data-layout-width={line.width}
                  data-layout-height={line.height} data-layout-baseline={line.baseline}>
                  {line.glyphs.map((glyph, index) => <use key={index} href={`#${outlineId(glyph.outlineId)}`}
                    transform={glyph.transform} fill={glyph.color} data-layout-outline-id={glyph.outlineId} />)}
                  {line.decorations.map((decoration, index) => <rect key={index}
                    x={decoration.x} y={decoration.y} width={decoration.width} height={decoration.thickness}
                    fill={decoration.color} data-layout-decoration="underline" />)}
                </g>)}
              </svg>
            </div>
            <figcaption>Page {page.number}</figcaption>
          </figure>)}
        </div></div>}
    </>}
  </div>;
}
