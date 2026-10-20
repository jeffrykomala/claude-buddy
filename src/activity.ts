/**
 * The two small things drawn over the pet itself (design §8.2, §8.3): the
 * activity label ("📖 reading main.rs") and the "…" dot you click to reopen
 * a reply that was minimized.
 */

/** The label above the pet's head while Claude works. */
export class ActivityLabel {
  readonly element: HTMLElement;

  constructor(doc: Document) {
    this.element = doc.createElement("div");
    this.element.className = "activity";
    this.element.hidden = true;
  }

  show(icon: string, label: string): void {
    this.element.textContent = `${icon} ${label}`;
    this.element.hidden = false;
  }

  hide(): void {
    this.element.hidden = true;
    this.element.textContent = "";
  }
}

/** The "…" dot. Looks different when the pet needs an answer. */
export class ReplyDot {
  readonly element: HTMLButtonElement;

  constructor(doc: Document, onClick: () => void) {
    this.element = doc.createElement("button");
    this.element.className = "dot";
    this.element.type = "button";
    this.element.title = "Show the reply";
    this.element.textContent = "…";
    this.element.hidden = true;
    this.element.addEventListener("click", (event) => {
      event.stopPropagation();
      onClick();
    });
  }

  show(needsAnswer: boolean): void {
    this.element.hidden = false;
    this.element.classList.toggle("attention", needsAnswer);
    this.element.textContent = needsAnswer ? "!" : "…";
    this.element.title = needsAnswer ? "I need your OK" : "Show the reply";
  }

  hide(): void {
    this.element.hidden = true;
  }
}
