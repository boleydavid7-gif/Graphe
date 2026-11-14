/** Read-only presentation transport. All edits and Unicode rules live in Rust. */
export type Alignment = 'left' | 'center' | 'right' | 'justify';

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
}

export interface RunView {
  readonly text: string;
  readonly format: CharacterView;
}

export interface ParagraphView {
  readonly id: string;
  readonly sectionId: string;
  readonly runs: readonly RunView[];
  readonly format: {
    readonly alignment: Alignment;
    readonly spacingBeforePt: number;
    readonly spacingAfterPt: number;
    readonly lineHeight: string;
    readonly indentLeftPt: number;
    readonly indentRightPt: number;
    readonly firstLinePt: number;
  };
}

export interface WriterView {
  readonly sessionId: string;
  /** Opaque session/revision handle; never convert it to a JavaScript number. */
  readonly revision: string;
  readonly documentId: string;
  readonly title: string;
  readonly paragraphs: readonly ParagraphView[];
  readonly selection: WriterSelection | null;
  readonly format: {
    readonly bold: boolean | null;
    readonly italic: boolean | null;
    readonly underline: boolean | null;
    readonly fontFamily: string | null;
    readonly fontSizePt: number | null;
    readonly alignment: Alignment | null;
  };
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  readonly wordCount: number;
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
  | ({ readonly type: 'insert_paragraph' } & EditOrigin)
  | ({ readonly type: 'insert_line_break' } & EditOrigin)
  | ({ readonly type: 'delete'; readonly direction: 'backward' | 'forward' } & EditOrigin)
  | ({ readonly type: 'format_character'; readonly bold?: boolean; readonly italic?: boolean;
       readonly underline?: boolean; readonly fontFamily?: string; readonly fontSizePt?: number } & EditOrigin)
  | ({ readonly type: 'format_paragraph'; readonly alignment: Alignment } & EditOrigin)
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
