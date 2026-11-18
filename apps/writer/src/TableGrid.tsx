import { memo } from 'react';
import type { LayoutPage } from './types';

/** Paint only Rust-provided cell rectangles; no browser table measurements. */
export const TableGrid = memo(function TableGrid({ page }: { readonly page: LayoutPage }) {
  return <g className="document-table-grid" fill="none" stroke="#808080" strokeWidth="32">
    {page.tableCells?.map((cell) => <rect key={cell.cellId} {...cell.rect}
      data-table-id={cell.tableId} data-table-row-id={cell.rowId} data-table-cell-id={cell.cellId} />)}
  </g>;
});
