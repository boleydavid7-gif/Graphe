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
