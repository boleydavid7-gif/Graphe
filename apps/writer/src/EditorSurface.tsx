import { forwardRef, useImperativeHandle, useLayoutEffect, useRef } from 'react';
import type { LayoutAction, LayoutMoveDirection, ParagraphView, TextPoint, WriterCommand, WriterSelection, WriterView } from './types';

export interface EditorSurfaceHandle {
  focus(): void;
  restoreSelection(): void;
  /** Synchronizes the input projection after a Rust visual-selection action. */
  syncView(view: WriterView): void;
  /** Reveals a deliberate selection without moving focus to the editor. */
  revealSelection?(view: WriterView): void;
}

export interface EditorSurfaceProps {
  readonly view: WriterView;
  readonly onCommand: (command: WriterCommand) => WriterView | undefined;
  readonly onError: (message: string) => void;
  readonly onCopy: () => string;
  readonly onCompositionChange?: (composing: boolean) => void;
  readonly disabled?: boolean;
  /** This DOM is only the native input/IME projection for Rust-rendered pages. */
  readonly inputOnly?: boolean;
  readonly onNavigate?: (action: LayoutAction) => WriterView | undefined;
  readonly onCompositionText?: (text: string) => void;
  readonly onPageScroll?: (direction: 'up' | 'down') => void;
}

interface CompositionOrigin {
  readonly revision: string;
  readonly selection: WriterSelection | null;
}

function sameSelection(left: WriterSelection | null, right: WriterSelection | null): boolean {
  return left === right || (left !== null && right !== null
    && left.anchor.paragraphId === right.anchor.paragraphId
    && left.anchor.utf16 === right.anchor.utf16
    && left.focus.paragraphId === right.focus.paragraphId
    && left.focus.utf16 === right.focus.utf16);
}

/** A presentation adapter, not a document model. Text offsets here are DOM UTF-16. */
class SurfaceController {
  private view: WriterView;
  private readonly paragraphs = new Map<string, HTMLParagraphElement>();
  private readonly tableNodes = new Map<string, HTMLElement>();
  private readonly identities = new WeakMap<Node, string>();
  private readonly signatures = new WeakMap<Node, string>();
  private composition: CompositionOrigin | null = null;
  private pendingInput: WriterCommand | null = null;
  private compositionTail = false;
  private tailTimer: ReturnType<typeof setTimeout> | undefined;
  private disposed = false;

  constructor(
    private readonly element: HTMLDivElement,
    private readonly props: () => EditorSurfaceProps,
  ) {
    this.view = props().view;
    this.reconcile(true);
    element.addEventListener('beforeinput', this.beforeInput);
    element.addEventListener('input', this.input);
    element.addEventListener('keydown', this.keyDown);
    element.addEventListener('paste', this.paste);
    element.addEventListener('copy', this.copy);
    element.addEventListener('cut', this.cut);
    element.addEventListener('drop', this.drop);
    element.addEventListener('compositionstart', this.compositionStart);
    element.addEventListener('compositionupdate', this.compositionUpdate);
    element.addEventListener('compositionend', this.compositionEnd);
    document.addEventListener('selectionchange', this.selectionChange);
  }

  dispose(): void {
    this.disposed = true;
    clearTimeout(this.tailTimer);
    this.element.removeEventListener('beforeinput', this.beforeInput);
    this.element.removeEventListener('input', this.input);
    this.element.removeEventListener('keydown', this.keyDown);
    this.element.removeEventListener('paste', this.paste);
    this.element.removeEventListener('copy', this.copy);
    this.element.removeEventListener('cut', this.cut);
    this.element.removeEventListener('drop', this.drop);
    this.element.removeEventListener('compositionstart', this.compositionStart);
    this.element.removeEventListener('compositionupdate', this.compositionUpdate);
    this.element.removeEventListener('compositionend', this.compositionEnd);
    document.removeEventListener('selectionchange', this.selectionChange);
  }

  update(view: WriterView): void {
    const changed = view.revision !== this.view.revision || view.sessionId !== this.view.sessionId;
    this.view = view;
    if (!this.composition) {
      this.reconcile();
      if (changed && document.activeElement === this.element) {
        this.restoreSelection();
        this.ensureCaretVisible();
      }
    }
  }

  focus(): void {
    this.element.focus({ preventScroll: true });
    this.restoreSelection();
  }

  restoreSelection(): void {
    if (this.composition || this.disposed) return;
    const selection = window.getSelection();
    if (!selection) return;
    const desired = this.view.selection;
    if (desired) {
      if (sameSelection(this.readSelection(), desired)) {
        this.alignProxyCaret();
        return;
      }
      const anchor = this.domPoint(desired.anchor);
      const focus = this.domPoint(desired.focus);
      if (anchor && focus) selection.setBaseAndExtent(anchor.node, anchor.offset, focus.node, focus.offset);
    } else if (!this.view.paragraphs.length && this.element.firstChild) {
      selection.setBaseAndExtent(this.element.firstChild, 0, this.element.firstChild, 0);
    }
    this.alignProxyCaret();
  }

  private alignProxyCaret(): void {
    if (!this.props().inputOnly || this.composition || document.activeElement !== this.element) return;
    const focus = this.view.selection?.focus;
    if (!focus) return;
    const point = this.domPoint(focus);
    if (!point) return;
    const range = document.createRange();
    range.setStart(point.node, point.offset);
    range.collapse(true);
    let rect = range.getBoundingClientRect();
    if (!rect.height) {
      const paragraph = this.paragraphs.get(focus.paragraphId);
      const terminal = paragraph?.lastElementChild;
      if (terminal instanceof HTMLBRElement) rect = terminal.getBoundingClientRect();
    }
    if (!rect.height) return;
    const bounds = this.element.getBoundingClientRect();
    // Align only the native input/candidate window. Visible page geometry and
    // cursor positions still come from Rust, never from this browser flow.
    const horizontal = rect.left - bounds.left;
    const vertical = rect.top - bounds.top;
    if (Math.abs(horizontal) > 1) this.element.scrollLeft += horizontal;
    if (Math.abs(vertical) > 1) this.element.scrollTop += vertical;
  }

  private ensureCaretVisible(): void {
    if (this.composition || this.disposed || document.activeElement !== this.element) return;
    if (this.props().inputOnly) {
      this.alignProxyCaret();
      return;
    }
    const focus = this.view.selection?.focus;
    if (!focus) return;
    const point = this.domPoint(focus);
    if (!point) return;
    const range = document.createRange();
    range.setStart(point.node, point.offset);
    range.collapse(true);
    let rect = range.getBoundingClientRect();
    if (!rect.height) {
      const paragraph = this.paragraphs.get(focus.paragraphId);
      if (!paragraph) return;
      const terminal = paragraph.lastElementChild;
      if (terminal instanceof HTMLBRElement && terminal.dataset.trailingSoftBreak === 'true'
        && focus.utf16 === (paragraph.textContent?.length ?? 0)) {
        // A collapsed range after terminal LF has no rect in Chromium. The
        // presentation-only BR occupies that exact final input line.
        rect = terminal.getBoundingClientRect();
      }
      if (!rect.height) {
        // Empty paragraphs do not reliably expose collapsed-range rectangles.
        // Use their first line, rather than scrolling the entire paragraph box.
        const bounds = paragraph.getBoundingClientRect();
        const lineHeight = Number.parseFloat(getComputedStyle(paragraph).lineHeight) || 20;
        rect = new DOMRect(bounds.left, bounds.top, 0, Math.min(bounds.height, lineHeight));
      }
    }
    const margin = 24;
    const top = rect.top < margin ? rect.top - margin
      : rect.bottom > window.innerHeight - margin ? rect.bottom - window.innerHeight + margin : 0;
    const left = rect.left < margin ? rect.left - margin
      : rect.right > window.innerWidth - margin ? rect.right - window.innerWidth + margin : 0;
    if (top || left) window.scrollBy({ top, left, behavior: 'instant' });
  }

  private renderParagraph(element: HTMLParagraphElement, paragraph: ParagraphView, force: boolean): void {
    const signature = JSON.stringify([paragraph, this.props().inputOnly ?? false]);
    if (!force && this.signatures.get(element) === signature) return;
    this.signatures.set(element, signature);
    const format = paragraph.format;
    if (force) element.style.cssText = '';
    element.style.textAlign = format.alignment;
    element.style.marginTop = `${format.spacingBeforePt}pt`;
    element.style.marginBottom = `${format.spacingAfterPt}pt`;
    element.style.lineHeight = format.lineHeight;
    element.style.paddingLeft = `${format.indentLeftPt}pt`;
    element.style.paddingRight = `${format.indentRightPt}pt`;
    element.style.textIndent = `${format.firstLinePt}pt`;
    element.dataset.sectionId = paragraph.sectionId;
    element.style.whiteSpace = 'pre-wrap';
    element.style.overflowWrap = 'anywhere';
    element.style.minHeight = '1.45em';
    element.style.width = '';
    if (this.props().inputOnly) {
      element.style.textAlign = 'left';
      element.style.margin = '0';
      element.style.padding = '0';
      element.style.textIndent = '0';
      element.style.whiteSpace = 'pre';
      element.style.overflowWrap = 'normal';
      element.style.minHeight = '20px';
      element.style.lineHeight = '20px';
      element.style.width = 'max-content';
    }

    // Preserve unchanged spans (and their text nodes) across ordinary updates.
    let index = 0;
    for (const run of paragraph.runs) {
      const runSignature = JSON.stringify(run);
      let span = element.childNodes[index];
      if (force || !(span instanceof HTMLSpanElement)) {
        const replacement = document.createElement('span');
        if (span) element.replaceChild(replacement, span);
        else element.append(replacement);
        span = replacement;
      }
      if (force || this.signatures.get(span) !== runSignature) {
        this.signatures.set(span, runSignature);
        const typedSpan = span as HTMLSpanElement;
        typedSpan.textContent = run.text;
        typedSpan.style.fontWeight = run.format.bold ? '700' : '400';
        typedSpan.style.fontStyle = run.format.italic ? 'italic' : 'normal';
        typedSpan.style.textDecorationLine = run.format.underline === 'none' ? 'none' : 'underline';
        typedSpan.style.textDecorationStyle = run.format.underline === 'double' ? 'double' : 'solid';
        typedSpan.style.fontFamily = run.format.fontFamily;
        typedSpan.style.fontSize = this.props().inputOnly ? '16px' : `${run.format.fontSizePt}pt`;
        typedSpan.style.color = run.format.color;
      }
      index += 1;
    }
    while (element.childNodes.length > index) element.lastChild?.remove();
    const text = element.textContent ?? '';
    if (!text || text.endsWith('\n')) {
      // Chromium needs a trailing presentation break to display an empty final
      // line. BR contributes no text to DOM-to-engine UTF-16 range mapping.
      const placeholder = document.createElement('br');
      if (text) placeholder.dataset.trailingSoftBreak = 'true';
      element.append(placeholder);
    }
  }

  private reconcile(force = false): void {
    if (this.composition || this.disposed) return;
    const active = new Set<string>();
    const rootChildren: Node[] = [];
    const tableChildren = new Map<HTMLElement, Node[]>();
    const activeTableNodes = new Set<string>();
    const container = (kind: 'table' | 'tbody' | 'tr' | 'td', id: string): HTMLElement => {
      const key = JSON.stringify([kind, id]);
      activeTableNodes.add(key);
      let node = this.tableNodes.get(key);
      if (!node) {
        node = document.createElement(kind);
        this.tableNodes.set(key, node);
      }
      if (!tableChildren.has(node)) tableChildren.set(node, []);
      return node;
    };
    const add = (parent: HTMLElement, child: Node) => {
      const children = tableChildren.get(parent)!;
      if (!children.includes(child)) children.push(child);
    };
    for (const paragraph of this.view.paragraphs) {
      active.add(paragraph.id);
      let element = this.paragraphs.get(paragraph.id);
      if (!element) {
        element = document.createElement('p');
        element.className = 'writer-paragraph';
        element.dataset.paragraphId = paragraph.id;
        this.paragraphs.set(paragraph.id, element);
        this.identities.set(element, paragraph.id);
      }
      this.renderParagraph(element, paragraph, force);
      const context = paragraph.tableCell;
      if (context) {
        const table = container('table', context.tableId);
        table.dataset.tableId = context.tableId;
        table.setAttribute('aria-label', 'Document table');
        const body = container('tbody', context.tableId);
        const row = container('tr', context.rowId);
        const cell = container('td', context.cellId);
        cell.dataset.tableCellId = context.cellId;
        cell.setAttribute('aria-label', `Row ${context.row + 1}, column ${context.column + 1}`);
        if (!rootChildren.includes(table)) rootChildren.push(table);
        add(table, body);
        add(body, row);
        add(row, cell);
        add(cell, element);
      } else rootChildren.push(element);
    }
    const place = (parent: HTMLElement, children: readonly Node[]) => {
      children.forEach((child, index) => {
        if (parent.childNodes[index] !== child) parent.insertBefore(child, parent.childNodes[index] ?? null);
      });
      while (parent.childNodes.length > children.length) parent.lastChild?.remove();
    };
    // Keep surviving paragraph/text nodes for native IME and selection. This is
    // a semantic input projection only; Rust supplies visible table geometry.
    for (const [parent, children] of tableChildren) place(parent, children);
    let index = rootChildren.length;
    if (!index) {
      let placeholder = this.element.firstChild;
      if (!(placeholder instanceof HTMLParagraphElement) || placeholder.dataset.empty !== 'true') {
        placeholder = document.createElement('p');
        (placeholder as HTMLParagraphElement).dataset.empty = 'true';
        (placeholder as HTMLParagraphElement).className = 'writer-paragraph';
        placeholder.appendChild(document.createElement('br'));
        this.element.insertBefore(placeholder, this.element.firstChild);
      } else if (force) {
        // Empty native documents deliberately have no Rust paragraph yet. An
        // uncancellable edit or canceled composition can still alter this DOM
        // placeholder; authoritative recovery must remove that transient text.
        placeholder.replaceChildren(document.createElement('br'));
      }
      index = 1;
    } else place(this.element, rootChildren);
    while (this.element.childNodes.length > index) this.element.lastChild?.remove();
    for (const [id, element] of this.paragraphs) {
      if (!active.has(id)) {
        element.remove();
        this.paragraphs.delete(id);
      }
    }
    for (const [key] of this.tableNodes) {
      if (!activeTableNodes.has(key)) this.tableNodes.delete(key);
    }
  }

  private point(node: Node | null, offset: number): TextPoint | null {
    if (!node || !this.element.contains(node)) return null;
    let paragraph: Node | null = node;
    while (paragraph && paragraph !== this.element && !this.identities.has(paragraph)) paragraph = paragraph.parentNode;
    const id = paragraph ? this.identities.get(paragraph) : undefined;
    if (!paragraph || !id) {
      // Container endpoints occur in native select-all/selection gestures. Map
      // only actual projected paragraph nodes, never child indices into Rust.
      if (!(node instanceof Element)) return null;
      const boundary = node.childNodes[offset];
      const next = boundary instanceof Element
        ? boundary.matches('[data-paragraph-id]') ? boundary : boundary.querySelector('[data-paragraph-id]') : null;
      if (next instanceof HTMLElement && next.dataset.paragraphId) return { paragraphId: next.dataset.paragraphId, utf16: 0 };
      const previous = node.childNodes[Math.min(offset, node.childNodes.length) - 1];
      const candidates = previous instanceof Element
        ? previous.matches('[data-paragraph-id]') ? [previous] : [...previous.querySelectorAll('[data-paragraph-id]')] : [];
      const last = candidates.at(-1);
      return last instanceof HTMLElement && last.dataset.paragraphId
        ? { paragraphId: last.dataset.paragraphId, utf16: last.textContent?.length ?? 0 } : null;
    }
    try {
      const range = document.createRange();
      range.setStart(paragraph, 0);
      range.setEnd(node, offset);
      // Text.length counts UTF-16 here, never cursor characters or graphemes.
      return { paragraphId: id, utf16: range.cloneContents().textContent?.length ?? 0 };
    } catch {
      return null;
    }
  }

  private readSelection(): WriterSelection | null {
    const selection = window.getSelection();
    if (!selection) return null;
    const anchor = this.point(selection.anchorNode, selection.anchorOffset);
    const focus = this.point(selection.focusNode, selection.focusOffset);
    return anchor && focus ? { anchor, focus } : null;
  }

  private domPoint(point: TextPoint): { node: Node; offset: number } | null {
    const paragraph = this.paragraphs.get(point.paragraphId);
    if (!paragraph) return null;
    const walker = document.createTreeWalker(paragraph, NodeFilter.SHOW_TEXT);
    let remaining = point.utf16;
    let text = walker.nextNode();
    while (text) {
      const length = text.textContent?.length ?? 0;
      if (remaining <= length) return { node: text, offset: remaining };
      remaining -= length;
      text = walker.nextNode();
    }
    return remaining === 0 ? { node: paragraph, offset: paragraph.childNodes.length } : null;
  }

  private origin(event?: InputEvent): { expectedRevision: string; selection?: WriterSelection } {
    // selectionchange is asynchronous. Capture both endpoints before flushing:
    // a fast next key must not reinterpret an interior DOM cursor with default
    // trailing affinity after a native backward arrow has moved it there.
    const nativeSelection = this.readSelection();
    let target: WriterSelection | null = null;
    const ranges = event?.getTargetRanges?.();
    const range = ranges?.length === 1 ? ranges[0] : undefined;
    if (range) {
      const anchor = this.point(range.startContainer, range.startOffset);
      const focus = this.point(range.endContainer, range.endOffset);
      if (anchor && focus) target = { anchor, focus };
    }
    const targetsNativeSelection = nativeSelection && (!target
      || sameSelection(target, nativeSelection)
      || sameSelection(target, { anchor: nativeSelection.focus, focus: nativeSelection.anchor }));
    if (targetsNativeSelection) this.selectionChange();
    // A distinct target range can represent spellcheck/autocorrect rather than
    // the caret. Keep that edit target intact for Rust's outward normalization.
    const selection = targetsNativeSelection
      ? this.view.selection ?? nativeSelection
      : target ?? nativeSelection ?? this.view.selection;
    return { expectedRevision: this.view.revision, ...(selection ? { selection } : {}) };
  }

  private apply(command: WriterCommand, restore = true): WriterView | undefined {
    const previousRevision = this.view.revision;
    const next = this.props().onCommand(command);
    if (next) this.view = next;
    this.reconcile();
    if (restore) {
      this.restoreSelection();
      if (next && next.revision !== previousRevision) this.ensureCaretVisible();
    }
    return next;
  }

  private selectionChange = (): void => {
    if (this.composition || this.props().disabled || document.activeElement !== this.element) return;
    const selection = this.readSelection();
    if (!selection || sameSelection(selection, this.view.selection)) return;
    const previous = this.view.selection;
    // A browser can stop inside a grapheme when its marks cross styled spans.
    // Infer logical movement without segmenting in JavaScript; Rust chooses the
    // corresponding boundary for the moving focus, including Shift-selection
    // contraction. Native visual/bidi navigation stays with the browser. Without
    // movement context Rust retains its default selection normalization policy.
    const affinity = previous && selection.focus.paragraphId === previous.focus.paragraphId
      && selection.focus.utf16 !== previous.focus.utf16
      ? selection.focus.utf16 < previous.focus.utf16 ? 'leading' : 'trailing'
      : undefined;
    const next = this.apply({
      type: 'set_selection', selection, ...(affinity ? { affinity } : {}), expectedRevision: this.view.revision,
    }, false);
    if (next && !sameSelection(selection, next.selection)) this.restoreSelection();
  };

  private inputCommand(event: InputEvent): WriterCommand | null {
    const origin = this.origin(event);
    switch (event.inputType) {
      case 'insertText':
      case 'insertReplacementText':
        return event.data !== null ? { type: 'replace_text', text: event.data, ...origin } : null;
      case 'insertParagraph':
        return { type: 'insert_paragraph', ...origin };
      case 'insertLineBreak':
        return { type: 'insert_line_break', ...origin };
      case 'deleteContentBackward':
        return { type: 'delete', direction: 'backward', ...origin };
      case 'deleteContentForward':
        return { type: 'delete', direction: 'forward', ...origin };
      case 'deleteByCut':
      case 'deleteByDrag':
      case 'deleteWordBackward':
      case 'deleteWordForward':
      case 'deleteSoftLineBackward':
      case 'deleteSoftLineForward':
      case 'deleteHardLineBackward':
      case 'deleteHardLineForward':
        return origin.selection && !sameSelection(origin.selection, {
          anchor: origin.selection.anchor, focus: origin.selection.anchor,
        }) ? { type: 'replace_text', text: '', ...origin } : null;
      case 'historyUndo':
        return { type: 'undo', expectedRevision: this.view.revision };
      case 'historyRedo':
        return { type: 'redo', expectedRevision: this.view.revision };
      case 'formatBold':
        return { type: 'format_character', bold: this.view.format.bold !== true, ...origin };
      case 'formatItalic':
        return { type: 'format_character', italic: this.view.format.italic !== true, ...origin };
      case 'formatUnderline':
        return { type: 'format_character', underline: this.view.format.underline !== true, ...origin };
      default:
        return null;
    }
  }

  private beforeInput = (event: InputEvent): void => {
    if (this.props().disabled) {
      if (event.cancelable) event.preventDefault();
      return;
    }
    if (this.composition || event.isComposing) return;
    if (this.compositionTail && (event.inputType === 'insertFromComposition'
      || event.inputType === 'insertText' || event.inputType === 'insertCompositionText')) {
      if (event.cancelable) event.preventDefault();
      this.reconcile(true);
      this.restoreSelection();
      return;
    }
    const command = this.inputCommand(event);
    if (!event.cancelable) {
      this.pendingInput = command;
      return;
    }
    event.preventDefault();
    if (command) this.apply(command);
    else this.props().onError('That browser editing action is not supported yet. The document is unchanged.');
  };

  private input = (): void => {
    if (this.composition) return;
    this.reconcile(true);
    if (this.compositionTail) {
      this.compositionTail = false;
      this.pendingInput = null;
      this.restoreSelection();
      return;
    }
    const command = this.pendingInput;
    this.pendingInput = null;
    if (command) this.apply(command);
    else {
      this.restoreSelection();
      this.props().onError('This browser edit could not be applied safely. Your document is unchanged; try typing or pasting again.');
    }
  };

  private keyDown = (event: KeyboardEvent): void => {
    if (event.isComposing || this.composition || event.keyCode === 229 || this.props().disabled) return;
    const origin = this.origin();
    const modifier = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    if (event.key === 'Tab' && !modifier && !event.altKey && this.view.table) {
      event.preventDefault();
      this.apply({ type: 'move_table_cell', direction: event.shiftKey ? 'backward' : 'forward', ...origin });
      return;
    }
    if (this.props().inputOnly && this.props().onNavigate) {
      let action: LayoutAction | undefined;
      let direction: LayoutMoveDirection | undefined;
      if (modifier && !event.altKey && key === 'a') action = { type: 'select_all' };
      else if (event.key === 'Home') direction = modifier ? 'document_start' : 'home';
      else if (event.key === 'End') direction = modifier ? 'document_end' : 'end';
      else if (event.metaKey && event.key === 'ArrowUp') direction = 'document_start';
      else if (event.metaKey && event.key === 'ArrowDown') direction = 'document_end';
      else if (event.metaKey && event.key === 'ArrowLeft') direction = 'home';
      else if (event.metaKey && event.key === 'ArrowRight') direction = 'end';
      else if (event.key.startsWith('Arrow')) {
        if (event.ctrlKey || event.altKey) {
          event.preventDefault();
          this.props().onError('Word-navigation shortcuts are not available yet. Use the arrow keys or select text on the page.');
          return;
        }
        direction = ({ ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down' } as const)[event.key as 'ArrowLeft'];
      } else if (event.key === 'PageUp' || event.key === 'PageDown') {
        event.preventDefault();
        if (this.props().onPageScroll) this.props().onPageScroll?.(event.key === 'PageUp' ? 'up' : 'down');
        else window.scrollBy({ top: (event.key === 'PageUp' ? -1 : 1) * window.innerHeight * 0.8 });
        return;
      }
      if (direction) action = { type: 'move', direction, extend: event.shiftKey };
      if (action) {
        event.preventDefault();
        const next = this.props().onNavigate?.(action);
        if (next) this.update(next);
        this.restoreSelection();
        return;
      }
    }
    let command: WriterCommand | null = null;
    if (modifier && !event.altKey) {
      if (key === 'z') command = { type: event.shiftKey ? 'redo' : 'undo', expectedRevision: this.view.revision };
      else if (key === 'y') command = { type: 'redo', expectedRevision: this.view.revision };
      else if (key === 'b') command = { type: 'format_character', bold: this.view.format.bold !== true, ...origin };
      else if (key === 'i') command = { type: 'format_character', italic: this.view.format.italic !== true, ...origin };
      else if (key === 'u') command = { type: 'format_character', underline: this.view.format.underline !== true, ...origin };
    } else if (!event.altKey) {
      if (event.key === 'Enter') command = { type: event.shiftKey ? 'insert_line_break' : 'insert_paragraph', ...origin };
      else if (event.key === 'Backspace') command = { type: 'delete', direction: 'backward', ...origin };
      else if (event.key === 'Delete') command = { type: 'delete', direction: 'forward', ...origin };
    }
    if (command) {
      event.preventDefault();
      this.apply(command);
    }
  };

  private paste = (event: ClipboardEvent): void => {
    event.preventDefault();
    if (this.composition || this.props().disabled) return;
    if (!event.clipboardData) {
      this.props().onError('The browser did not provide clipboard text. Paste was not applied.');
      return;
    }
    if (!Array.from(event.clipboardData.types).includes('text/plain')) {
      this.props().onError('This prototype pastes plain text. The clipboard did not contain plain text, so your document is unchanged.');
      return;
    }
    this.apply({ type: 'replace_text', text: event.clipboardData.getData('text/plain'), ...this.origin() });
  };

  private copySelection(event: ClipboardEvent): WriterView | undefined {
    event.preventDefault();
    if (this.composition || this.props().disabled) {
      return;
    }
    const origin = this.origin();
    if (!origin.selection || sameSelection(origin.selection, {
      anchor: origin.selection.anchor, focus: origin.selection.anchor,
    })) return;
    if (!event.clipboardData) {
      this.props().onError('The browser did not provide clipboard access. Nothing was copied or cut.');
      return;
    }
    const normalized = this.apply({
      type: 'set_selection', selection: origin.selection, expectedRevision: origin.expectedRevision,
    });
    if (!normalized?.selection) return;
    try {
      // The native DOM adds visual blank lines between paragraph nodes. Only
      // Rust knows the structural selection and its one-LF clipboard policy.
      event.clipboardData.setData('text/plain', this.props().onCopy());
    } catch {
      this.props().onError('The browser could not copy the selected text. Nothing was cut.');
      return;
    }
    return normalized;
  }

  private copy = (event: ClipboardEvent): void => {
    this.copySelection(event);
  };

  private cut = (event: ClipboardEvent): void => {
    const normalized = this.copySelection(event);
    if (!normalized?.selection) return;
    this.apply({ type: 'replace_text', text: '', selection: normalized.selection, expectedRevision: normalized.revision });
  };

  private drop = (event: DragEvent): void => {
    event.preventDefault();
    this.props().onError('Drag and drop editing is not supported yet. Paste plain text instead.');
  };

  private compositionStart = (): void => {
    if (this.props().disabled) return;
    this.pendingInput = null;
    this.compositionTail = false;
    clearTimeout(this.tailTimer);
    const origin = this.origin();
    this.composition = { revision: origin.expectedRevision, selection: origin.selection ?? null };
    this.props().onCompositionChange?.(true);
    this.props().onCompositionText?.('');
  };

  private compositionUpdate = (event: CompositionEvent): void => {
    if (this.composition) this.props().onCompositionText?.(event.data);
  };

  private compositionEnd = (event: CompositionEvent): void => {
    const origin = this.composition;
    if (!origin) return;
    this.composition = null;
    this.reconcile(true);
    if (event.data) {
      this.apply({
        type: 'replace_text', text: event.data, expectedRevision: origin.revision,
        ...(origin.selection ? { selection: origin.selection } : {}),
      });
    } else this.restoreSelection();
    // Some browsers emit a final input after compositionend in this same task.
    // Expire at the next task so the next genuine key is never swallowed.
    this.compositionTail = true;
    clearTimeout(this.tailTimer);
    this.tailTimer = setTimeout(() => { this.compositionTail = false; }, 0);
    this.props().onCompositionChange?.(false);
    this.props().onCompositionText?.('');
  };
}

/** Native selection and composition are transient; only Rust snapshots persist. */
export const EditorSurface = forwardRef<EditorSurfaceHandle, EditorSurfaceProps>(function EditorSurface(props, ref) {
  const element = useRef<HTMLDivElement>(null);
  const latest = useRef(props);
  const controller = useRef<SurfaceController | null>(null);
  useLayoutEffect(() => {
    latest.current = props;
    controller.current?.update(props.view);
  });
  useLayoutEffect(() => {
    if (!element.current) return;
    const surface = new SurfaceController(element.current, () => latest.current);
    controller.current = surface;
    return () => {
      surface.dispose();
      controller.current = null;
    };
  }, []);
  useImperativeHandle(ref, () => ({
    focus: () => controller.current?.focus(),
    restoreSelection: () => controller.current?.restoreSelection(),
    syncView: (view) => controller.current?.update(view),
  }), []);
  return <div
    ref={element}
    className="editor-surface"
    style={props.inputOnly ? { width: '100%', height: '100%', minHeight: 0, padding: 0,
      margin: 0, border: 0, overflow: 'hidden', lineHeight: '20px', fontSize: '16px' } : undefined}
    contentEditable={!props.disabled}
    suppressContentEditableWarning
    role="textbox"
    aria-label="Document content"
    aria-multiline="true"
    aria-readonly={props.disabled || undefined}
    spellCheck
    data-testid="editor"
  />;
});
