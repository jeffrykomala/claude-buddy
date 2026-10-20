/**
 * The speech bubble (design §9.2): a text box (Return sends, Shift+Return
 * adds a line, Esc closes), the streaming reply with light Markdown, an
 * approval question with Allow / Deny, a picker for recent conversations,
 * and header buttons for a new conversation, minimize and close.
 *
 * Plain DOM, no framework: the page is tiny and lives in a transparent
 * window.
 */

import type { ApprovalRequest, RecentEntry } from "./ipc";
import { escapeHtml, renderMarkdown } from "./markdown";

export interface BubbleSize {
  readonly width: number;
  readonly height: number;
}

/** Just the header and the text box. */
export const COMPACT_SIZE: BubbleSize = { width: 340, height: 150 };
/** Room for a reply, an approval or the picker as well. */
export const TALL_SIZE: BubbleSize = { width: 340, height: 300 };

export interface BubbleHandlers {
  onSend(text: string): void;
  onStop(): void;
  onMinimize(): void;
  onClose(): void;
  onNew(): void;
  onAnswer(id: string, allow: boolean): void;
  onPick(id: string): void;
  onSendAsNew(): void;
  /** The bubble needs more or less room. */
  onSizeChange(size: BubbleSize): void;
}

function element<K extends keyof HTMLElementTagNameMap>(
  doc: Document,
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const el = doc.createElement(tag);
  el.className = className;
  if (text !== undefined) {
    el.textContent = text;
  }
  return el;
}

export class Bubble {
  readonly element: HTMLElement;

  private readonly doc: Document;
  private readonly reply: HTMLElement;
  private readonly status: HTMLElement;
  private readonly footer: HTMLElement;
  private readonly extras: HTMLElement;
  private readonly notice: HTMLElement;
  private readonly input: HTMLTextAreaElement;
  private readonly sendButton: HTMLButtonElement;

  private replyText = "";
  private busy = false;
  private tall = false;
  private open = false;

  constructor(
    doc: Document,
    private readonly handlers: BubbleHandlers,
  ) {
    this.doc = doc;
    this.element = element(doc, "section", "bubble");
    this.element.hidden = true;

    const header = element(doc, "header", "bubble-header");
    header.append(
      element(doc, "span", "bubble-title", "Buddy"),
      this.headerButton("🆕", "New conversation", () => handlers.onNew()),
      this.headerButton("–", "Minimize", () => handlers.onMinimize()),
      this.headerButton("×", "Close", () => handlers.onClose()),
    );

    this.notice = element(doc, "div", "bubble-notice");
    this.notice.hidden = true;
    this.reply = element(doc, "div", "bubble-reply");
    this.reply.hidden = true;
    this.status = element(doc, "div", "bubble-status");
    this.status.hidden = true;
    this.extras = element(doc, "div", "bubble-extras");
    this.extras.hidden = true;
    this.footer = element(doc, "div", "bubble-footer");
    this.footer.hidden = true;

    const body = element(doc, "div", "bubble-body");
    body.append(this.reply, this.extras, this.status, this.footer);

    this.input = element(doc, "textarea", "bubble-input");
    this.input.rows = 2;
    this.input.placeholder = "Ask me anything…";
    this.input.addEventListener("keydown", (event) => this.onKey(event));
    this.sendButton = element(doc, "button", "bubble-send", "Send");
    this.sendButton.type = "button";
    this.sendButton.addEventListener("click", () => this.onSendClicked());
    const inputRow = element(doc, "div", "bubble-input-row");
    inputRow.append(this.input, this.sendButton);

    this.element.append(header, this.notice, body, inputRow);
  }

  // --------------------------------------------------------------- state

  get isOpen(): boolean {
    return this.open;
  }

  /** Whether there is a reply (or an error) to show again. */
  hasReply(): boolean {
    return this.replyText !== "" || !this.footer.hidden;
  }

  size(): BubbleSize {
    return this.tall ? TALL_SIZE : COMPACT_SIZE;
  }

  show(): void {
    this.open = true;
    this.element.hidden = false;
    this.handlers.onSizeChange(this.size());
  }

  hide(): void {
    this.open = false;
    this.element.hidden = true;
  }

  focusInput(): void {
    this.input.focus();
  }

  /** While a run is going the box is locked and Send becomes Stop. */
  setBusy(busy: boolean): void {
    this.busy = busy;
    this.input.disabled = busy;
    this.sendButton.textContent = busy ? "Stop" : "Send";
    this.sendButton.classList.toggle("stop", busy);
  }

  // --------------------------------------------------------------- reply

  /** A new message is going out: start a fresh reply. */
  startReply(): void {
    this.replyText = "";
    this.reply.innerHTML = "";
    this.reply.classList.remove("error");
    this.reply.hidden = false;
    this.footer.hidden = true;
    this.clearExtras();
    this.setStatus("thinking…");
    this.setTall(true);
  }

  appendReply(text: string): void {
    this.replyText += text;
    this.renderReply();
    this.setStatus(null);
  }

  setStatus(text: string | null): void {
    this.status.hidden = text === null;
    this.status.textContent = text ?? "";
  }

  /** The run is over; `text` is the final text (or the error message). */
  finishReply(text: string, isError: boolean, model: string | null): void {
    this.setStatus(null);
    this.reply.hidden = false;
    if (isError) {
      const error = element(this.doc, "div", "bubble-error");
      error.textContent = text;
      this.reply.append(error);
    } else if (this.replyText === "") {
      this.replyText = text;
      this.renderReply();
    }
    this.footer.hidden = model === null;
    this.footer.textContent = model ?? "";
    this.setTall(true);
    this.scrollToEnd();
  }

  private renderReply(): void {
    this.reply.innerHTML = renderMarkdown(this.replyText);
    this.scrollToEnd();
  }

  private scrollToEnd(): void {
    this.reply.scrollTop = this.reply.scrollHeight;
  }

  // -------------------------------------------------------------- extras

  showApproval(request: ApprovalRequest): void {
    this.clearExtras();
    const box = element(this.doc, "div", `bubble-approval ${request.kind}`);
    box.append(element(this.doc, "strong", "approval-headline", request.headline));
    if (request.detail !== null) {
      box.append(element(this.doc, "pre", "approval-detail", request.detail));
    }
    const buttons = element(this.doc, "div", "approval-buttons");
    buttons.append(
      this.actionButton("Allow", "allow", () =>
        this.handlers.onAnswer(request.id, true),
      ),
      this.actionButton("Deny", "deny", () =>
        this.handlers.onAnswer(request.id, false),
      ),
    );
    box.append(buttons);
    this.setExtras(box);
  }

  /** Removes an approval question once it has been answered. */
  clearApproval(): void {
    if (this.extras.querySelector(".bubble-approval") !== null) {
      this.clearExtras();
    }
  }

  showPicker(entries: readonly RecentEntry[]): void {
    this.clearExtras();
    const box = element(this.doc, "div", "bubble-picker");
    if (entries.length === 0) {
      box.append(
        element(this.doc, "p", "picker-empty", "No recent conversations yet."),
      );
    }
    for (const entry of entries) {
      const row = element(this.doc, "button", "picker-row");
      row.type = "button";
      row.innerHTML = `${escapeHtml(entry.title)} <span class="when">${escapeHtml(entry.when)}</span>`;
      row.addEventListener("click", () => this.handlers.onPick(entry.id));
      box.append(row);
    }
    this.setExtras(box);
  }

  showLost(): void {
    this.clearExtras();
    this.setStatus(null);
    const box = element(this.doc, "div", "bubble-lost");
    box.append(
      element(this.doc, "p", "lost-text", "I've forgotten that conversation."),
      this.actionButton("Send as a new conversation", "send-new", () =>
        this.handlers.onSendAsNew(),
      ),
    );
    this.setExtras(box);
  }

  /** A short line above the reply ("Resumed: …"). */
  showNotice(message: string): void {
    this.notice.textContent = message;
    this.notice.hidden = false;
  }

  /** A new conversation: forget everything shown. */
  reset(): void {
    this.replyText = "";
    this.reply.innerHTML = "";
    this.reply.hidden = true;
    this.footer.hidden = true;
    this.notice.hidden = true;
    this.setStatus(null);
    this.clearExtras();
    this.setTall(false);
  }

  private setExtras(content: HTMLElement): void {
    this.extras.replaceChildren(content);
    this.extras.hidden = false;
    this.setTall(true);
  }

  private clearExtras(): void {
    this.extras.replaceChildren();
    this.extras.hidden = true;
  }

  private setTall(tall: boolean): void {
    if (this.tall === tall) {
      return;
    }
    this.tall = tall;
    this.element.classList.toggle("tall", tall);
    if (this.open) {
      this.handlers.onSizeChange(this.size());
    }
  }

  // ------------------------------------------------------------- input

  private onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      this.handlers.onClose();
      return;
    }
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      this.submit();
    }
  }

  private onSendClicked(): void {
    if (this.busy) {
      this.handlers.onStop();
      return;
    }
    this.submit();
  }

  private submit(): void {
    const text = this.input.value.trim();
    if (this.busy || text === "") {
      return;
    }
    this.input.value = "";
    this.handlers.onSend(text);
  }

  // ----------------------------------------------------------- helpers

  private headerButton(
    label: string,
    title: string,
    onClick: () => void,
  ): HTMLButtonElement {
    const button = element(this.doc, "button", "header-button", label);
    button.type = "button";
    button.title = title;
    button.addEventListener("click", onClick);
    return button;
  }

  private actionButton(
    label: string,
    className: string,
    onClick: () => void,
  ): HTMLButtonElement {
    const button = element(this.doc, "button", `action ${className}`, label);
    button.type = "button";
    button.addEventListener("click", onClick);
    return button;
  }
}
