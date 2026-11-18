/** Read-only presentation transport. All edits and Unicode rules live in Rust. */
export type Alignment = 'left' | 'center' | 'right' | 'justify';
export type ListKind = 'bullet' | 'numbered';

/** Native and external adapters; PDF is export-only. */
export type DocumentFormat = 'writer' | 'docx' | 'odt' | 'rtf' | 'txt' | 'pdf';

/** Rust-reported losses for an exact document revision, before Save As. */
export interface ExportAssessment {
  readonly sessionId: string;
  readonly documentId: string;
  readonly revision: string;
  readonly format: DocumentFormat;
  readonly warnings: readonly {
    readonly code: string;
    readonly message: string;
  }[];
}

export interface TextPoint {
  readonly paragraphId: string;
  readonly utf16: number;
}

export interface WriterSelection {
  readonly anchor: TextPoint;
  readonly focus: TextPoint;
}

export interface CharacterView {
  readonly bold: boolean;
  readonly italic: boolean;
  readonly underline: 'none' | 'single' | 'double';
  readonly fontFamily: string;
  readonly fontSizePt: number;
  readonly color: string;
  readonly hyperlink: string | null;
}

export interface RunView {
  readonly text: string;
  readonly format: CharacterView;
}

/** Rust-owned table location, kept out of JavaScript text editing logic. */
export interface TableCellContext {
  readonly tableId: string;
  readonly rowId: string;
  readonly cellId: string;
  readonly row: number;
  readonly column: number;
  readonly rowCount: number;
  readonly columnCount: number;
}

export type TableAction = 'insert_row_above' | 'insert_row_below' | 'delete_row'
  | 'insert_column_before' | 'insert_column_after' | 'delete_column' | 'delete_table';

/** Read-only image metadata. Original raster bytes and sizing rules live in Rust. */
export interface ImageView {
  readonly id: string;
  readonly sectionId: string;
  readonly widthPt: number;
  readonly heightPt: number;
  readonly pixelWidth: number;
  readonly pixelHeight: number;
  readonly altText: string;
  readonly alignment: 'left' | 'center' | 'right';
  readonly colorApproximation?: boolean;
  readonly orientationApplied?: boolean;
}

export interface BodyBlockView {
  readonly kind: 'paragraph' | 'table' | 'image';
  readonly id: string;
  readonly sectionId: string;
}

export interface ParagraphView {
  readonly id: string;
  readonly sectionId: string;
  readonly styleId: string | null;
  readonly list: ListKind | null;
  readonly tableCell?: TableCellContext | null;
  readonly runs: readonly RunView[];
  readonly format: {
    readonly alignment: Alignment;
    readonly spacingBeforePt: number;
    readonly spacingAfterPt: number;
    readonly lineHeight: string;
    readonly indentLeftPt: number;
    readonly indentRightPt: number;
    readonly firstLinePt: number;
    readonly pageBreakBefore: boolean;
    readonly keepTogether: boolean;
    readonly keepWithNext: boolean;
  };
}

export interface WriterView {
  readonly sessionId: string;
  /** Opaque session/revision handle; never convert it to a JavaScript number. */
  readonly revision: string;
  readonly documentId: string;
  readonly title: string;
  readonly paragraphs: readonly ParagraphView[];
  readonly styles: readonly { readonly id: string; readonly name: string }[];
  readonly selection: WriterSelection | null;
  readonly table?: TableCellContext | null;
  readonly pageSetup?: {
    readonly sectionId: string;
    readonly sectionIndex: number;
    readonly sectionCount: number;
    readonly pageSize: { readonly width: number; readonly height: number; readonly orientation: 'portrait' | 'landscape' | null };
    readonly pageMargins: { readonly top: number; readonly right: number; readonly bottom: number;
      readonly left: number; readonly header: number; readonly footer: number; readonly gutter: number };
  } | null;
  readonly images?: readonly ImageView[];
  readonly selectedImageId?: string | null;
  readonly bodyBlocks?: readonly BodyBlockView[];
  readonly format: {
    readonly bold: boolean | null;
    readonly italic: boolean | null;
    readonly underline: boolean | null;
    readonly fontFamily: string | null;
    readonly fontSizePt: number | null;
    readonly alignment: Alignment | null;
    readonly spacingBeforePt: number | null;
    readonly spacingAfterPt: number | null;
    readonly lineSpacingMultiple: number | null;
    readonly indentLeftIn: number | null;
    readonly indentRightIn: number | null;
    readonly firstLineIn: number | null;
    readonly styleId: string | null;
    readonly styleMixed: boolean;
    readonly list: ListKind | null;
    readonly listMixed: boolean;
    readonly hyperlink: string | null;
  };
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  readonly wordCount: number;
  /** Runtime approximation notices, derived again from the retained source on reopen. */
  readonly importWarnings?: CompatibilityReport;
  /** Immutable retained input, separate from current content and undo history. */
  readonly originalSource: {
    readonly format: string;
    readonly filename: string;
    readonly byteLength: number;
  } | null;
}

interface EditOrigin {
  readonly expectedRevision?: string;
  readonly selection?: WriterSelection;
}

export type WriterCommand =
  | { readonly type: 'new_document' }
  | { readonly type: 'fork_document' }
  | { readonly type: 'set_title'; readonly title: string; readonly expectedRevision?: string }
  | { readonly type: 'set_selection'; readonly selection: WriterSelection | null;
      readonly affinity?: 'leading' | 'trailing'; readonly expectedRevision?: string }
  | ({ readonly type: 'replace_text'; readonly text: string } & EditOrigin)
  | { readonly type: 'replace_all'; readonly query: string; readonly text: string; readonly expectedRevision?: string }
  | ({ readonly type: 'insert_paragraph' } & EditOrigin)
  | ({ readonly type: 'insert_line_break' } & EditOrigin)
  | ({ readonly type: 'insert_table'; readonly rows: number; readonly columns: number } & EditOrigin)
  | ({ readonly type: 'table_action'; readonly action: TableAction } & EditOrigin)
  | ({ readonly type: 'move_table_cell'; readonly direction: 'forward' | 'backward' } & EditOrigin)
  | { readonly type: 'select_image'; readonly imageId: string | null; readonly expectedRevision?: string }
  | { readonly type: 'resize_image'; readonly widthPt?: number; readonly heightPt?: number;
      readonly lockAspect?: boolean; readonly expectedRevision?: string }
  | { readonly type: 'set_image_alt_text'; readonly altText: string; readonly expectedRevision?: string }
  | { readonly type: 'set_image_alignment'; readonly alignment: 'left' | 'center' | 'right'; readonly expectedRevision?: string }
  | { readonly type: 'update_image'; readonly widthPt?: number; readonly heightPt?: number; readonly lockAspect?: boolean;
      readonly altText?: string; readonly alignment?: 'left' | 'center' | 'right'; readonly expectedRevision?: string }
  | { readonly type: 'delete_image'; readonly expectedRevision?: string }
  | { readonly type: 'move_image_cursor'; readonly direction: 'forward' | 'backward'; readonly expectedRevision?: string }
  | ({ readonly type: 'delete'; readonly direction: 'backward' | 'forward' } & EditOrigin)
  | ({ readonly type: 'format_character'; readonly bold?: boolean; readonly italic?: boolean;
       readonly underline?: boolean; readonly fontFamily?: string; readonly fontSizePt?: number } & EditOrigin)
  | ({ readonly type: 'format_paragraph'; readonly alignment: Alignment } & EditOrigin)
  | { readonly type: 'set_page_setup'; readonly sectionId: string;
      readonly pageSize: NonNullable<WriterView['pageSetup']>['pageSize'];
      readonly pageMargins: NonNullable<WriterView['pageSetup']>['pageMargins'];
      readonly expectedRevision?: string }
  | ({ readonly type: 'insert_page_break' } & EditOrigin)
  | ({ readonly type: 'insert_section_break' } & EditOrigin)
  | { readonly type: 'remove_section_break'; readonly sectionId: string;
      readonly confirmLayoutChange: boolean; readonly expectedRevision?: string }
  | ({ readonly type: 'set_page_break_before'; readonly enabled: boolean } & EditOrigin)
  | ({ readonly type: 'set_paragraph_pagination';
      readonly keepTogether?: boolean; readonly keepWithNext?: boolean } & EditOrigin)
  | ({ readonly type: 'set_paragraph_metrics';
       readonly spacingBeforePt?: number; readonly spacingAfterPt?: number;
       readonly lineSpacingMultiple?: number;
       readonly indentLeftIn?: number; readonly indentRightIn?: number;
       readonly firstLineIn?: number } & EditOrigin)
  | ({ readonly type: 'set_list'; readonly list: ListKind | null } & EditOrigin)
  | ({ readonly type: 'set_hyperlink'; readonly url: string | null; readonly text?: string } & EditOrigin)
  | ({ readonly type: 'assign_style'; readonly styleId: string | null } & EditOrigin)
  | { readonly type: 'undo'; readonly expectedRevision?: string }
  | { readonly type: 'redo'; readonly expectedRevision?: string };

export interface EngineFailure {
  readonly code: string;
  readonly message: string;
  readonly compatibility?: CompatibilityReport;
}

export interface CompatibilityReport {
  readonly diagnostics: readonly {
    readonly kind: string;
    readonly part: string;
    readonly path: string;
    readonly message: string;
  }[];
  readonly omittedDiagnostics: number;
}

export type EngineResponse =
  | { readonly ok: true; readonly view: WriterView }
  | { readonly ok: false; readonly error: EngineFailure };

/** Physical dimensions and margins use native integer twips (1/20 point). */
export interface LayoutOptions {
  readonly pageSize?: {
    readonly width: number;
    readonly height: number;
    readonly orientation?: 'portrait' | 'landscape' | null;
  };
  readonly pageMargins?: {
    readonly top: number;
    readonly right: number;
    readonly bottom: number;
    readonly left: number;
    readonly header: number;
    readonly footer: number;
    readonly gutter: number;
  };
  readonly fontFamily?: string;
  /** Font size uses native half-points: 24 means 12 points. */
  readonly fontSize?: number;
  readonly lineSpacing?:
    | { readonly kind: 'multiple'; readonly value: number }
    | { readonly kind: 'exact' | 'at_least'; readonly height: number };
  readonly spacingAfter?: number;
}

/** Every geometry number is a signed fixed unit of 1/64 point, not CSS pixels. */
export interface LayoutRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface LayoutTextRange {
  readonly startUtf8: number;
  readonly endUtf8: number;
  readonly startUtf16: number;
  readonly endUtf16: number;
  readonly startGrapheme: number;
  readonly endGrapheme: number;
}

export interface LayoutGlyph {
  readonly outlineId: number;
  readonly x: number;
  readonly y: number;
  readonly fontSize: number;
  /** Rust-computed SVG transform; the browser must not reshape the glyph. */
  readonly transform: string;
  readonly color: string;
}

export interface LayoutGlyphOutline {
  readonly id: number;
  readonly fontId: string;
  readonly glyphId: number;
  readonly unitsPerEm: number;
  /** Trusted typed font outlines serialized by Rust, never raw embedded SVG. */
  readonly path: string;
}

export interface LayoutCaret {
  readonly utf8: number;
  readonly utf16: number;
  readonly grapheme: number;
  readonly x: number;
  readonly y: number;
  readonly height: number;
  readonly affinity: 'leading' | 'trailing';
}

export interface LayoutDecoration {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly thickness: number;
  readonly color: string;
}

export interface LayoutLine {
  readonly paragraphId: string;
  readonly lineIndex: number;
  readonly text: string;
  readonly textRange: LayoutTextRange;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly baseline: number;
  readonly glyphs: readonly LayoutGlyph[];
  readonly carets: readonly LayoutCaret[];
  readonly decorations: readonly LayoutDecoration[];
}

export interface LayoutPage {
  readonly number: number;
  readonly sectionId: string;
  readonly width: number;
  readonly height: number;
  readonly contentRect: LayoutRect;
  readonly lines: readonly LayoutLine[];
  readonly images?: readonly { readonly imageId: string; readonly rect: LayoutRect }[];
  readonly tableCells?: readonly {
    readonly tableId: string;
    readonly rowId: string;
    readonly cellId: string;
    readonly rect: LayoutRect;
    readonly paragraphIds: readonly string[];
  }[];
}

export interface LayoutDiagnostic {
  readonly kind: string;
  readonly message: string;
  readonly paragraphId?: string | null;
}

export interface DocumentLayout {
  readonly documentId: string;
  readonly fontCatalogId: string;
  readonly unitsPerPoint: 64;
  readonly pages: readonly LayoutPage[];
  readonly outlines: readonly LayoutGlyphOutline[];
  readonly diagnostics: readonly LayoutDiagnostic[];
}

/** Geometry is tied to a live Rust session and opaque document revision. */
export interface LayoutView {
  readonly sessionId: string;
  readonly revision: string;
  readonly documentId: string;
  readonly layout: DocumentLayout;
}

/** A visual request interpreted against the Rust layout, never DOM text metrics. */
export type LayoutAction =
  | { readonly type: 'hit'; readonly pageNumber: number; readonly x: number; readonly y: number; readonly extend?: boolean }
  | { readonly type: 'select_word'; readonly pageNumber: number; readonly x: number; readonly y: number }
  | { readonly type: 'drag_endpoint'; readonly endpoint: 'anchor' | 'focus'; readonly pageNumber: number; readonly x: number; readonly y: number }
  | { readonly type: 'move'; readonly direction: LayoutMoveDirection; readonly extend?: boolean }
  | { readonly type: 'select_all' };

export type LayoutMoveDirection = 'left' | 'right' | 'up' | 'down' | 'home' | 'end'
  | 'document_start' | 'document_end';

export interface LayoutPosition extends TextPoint {
  readonly utf8: number;
  readonly grapheme: number;
  readonly affinity: 'leading' | 'trailing';
}

export interface CaretGeometry {
  readonly position: LayoutPosition;
  readonly pageNumber: number;
  readonly lineIndex: number;
  readonly rect: LayoutRect;
}

export interface SelectionRectangle {
  readonly pageNumber: number;
  readonly paragraphId: string;
  readonly lineIndex: number;
  readonly rect: LayoutRect;
}

export interface SelectionGeometry {
  readonly anchor: CaretGeometry;
  readonly focus: CaretGeometry;
  readonly rectangles: readonly SelectionRectangle[];
}

export interface LayoutSelectionView {
  readonly sessionId: string;
  readonly revision: string;
  readonly documentId: string;
  readonly geometry: SelectionGeometry | null;
  readonly preferredX: number | null;
}

export interface LayoutActionView extends LayoutSelectionView {
  readonly view: WriterView;
}

/** Literal matches and their validated selections are computed by Rust. */
export interface FindSnapshot {
  readonly sessionId: string;
  readonly documentId: string;
  readonly revision: string;
  readonly matches: readonly { readonly selection: WriterSelection }[];
  readonly truncated: boolean;
}
