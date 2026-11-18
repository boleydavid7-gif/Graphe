import type { ImageView, LayoutPage } from './types';

/** Rust supplies every image rectangle and canonical raster; SVG only paints it. */
export function ImageLayer({ page, images, urls, selectedId, onSelect }: {
  readonly page: LayoutPage;
  readonly images: readonly ImageView[];
  readonly urls: ReadonlyMap<string, string>;
  readonly selectedId?: string | null;
  readonly onSelect?: (id: string) => void;
}) {
  return <g className="document-images">{page.images?.map((placement) => {
    const image = images.find((item) => item.id === placement.imageId);
    if (!image) return null;
    const selected = selectedId === image.id;
    const url = urls.get(image.id);
    return <g key={image.id} style={{ cursor: onSelect ? 'pointer' : undefined, touchAction: 'pan-x pan-y pinch-zoom' }}
      onPointerDown={onSelect ? (event) => event.stopPropagation() : undefined}
      onPointerMove={onSelect ? (event) => event.stopPropagation() : undefined}
      onPointerUp={onSelect ? (event) => event.stopPropagation() : undefined}
      onClick={onSelect ? (event) => { event.stopPropagation(); onSelect(image.id); } : undefined}>
      <title>{image.altText || 'Document image'}</title>
      {url ? <image {...placement.rect} href={url} preserveAspectRatio="none" style={{ pointerEvents: onSelect ? 'all' : 'none' }}
        data-document-image-id={image.id} />
        : <rect {...placement.rect} fill="#eee9e1" data-image-loading={image.id} />}
      {onSelect && <rect {...placement.rect} fill="transparent" stroke={selected ? '#b78e37' : 'none'}
        strokeWidth={64} data-image-selected={selected ? 'true' : undefined}
        style={{ pointerEvents: 'none' }} />}
    </g>;
  })}</g>;
}
