/**
 * Light Markdown for the speech bubble: paragraphs, bold, italic, inline
 * code, code blocks, bullet and numbered lists. Everything else (links,
 * images, raw HTML) stays plain text, and the input is escaped first, so the
 * result is safe to assign to `innerHTML`.
 */

const ESCAPES: Readonly<Record<string, string>> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (char) => ESCAPES[char] ?? char);
}

const CODE_SPAN = /(`[^`\n]+`)/;

/** Bold, italic and code inside one line of already-plain text. */
function renderInline(line: string): string {
  return line
    .split(CODE_SPAN)
    .map((part) => {
      if (part.length > 2 && part.startsWith("`") && part.endsWith("`")) {
        return `<code>${escapeHtml(part.slice(1, -1))}</code>`;
      }
      return escapeHtml(part)
        .replace(/\*\*([^*\n]+?)\*\*/g, "<strong>$1</strong>")
        .replace(/(^|[^*])\*([^*\s][^*\n]*?)\*(?!\*)/g, "$1<em>$2</em>");
    })
    .join("");
}

const BULLET = /^\s*[-*]\s+(.*)$/;
const NUMBERED = /^\s*\d+[.)]\s+(.*)$/;
const HEADING = /^#{1,6}\s+(.*)$/;
const FENCE = /^\s*```/;

type Block =
  | { kind: "paragraph"; lines: string[] }
  | { kind: "ul" | "ol"; items: string[] }
  | { kind: "code"; lines: string[] };

function parseBlocks(source: string): Block[] {
  const blocks: Block[] = [];
  let current: Block | null = null;
  let inCode = false;

  const flush = (): void => {
    if (current !== null) {
      blocks.push(current);
      current = null;
    }
  };

  for (const line of source.replace(/\r\n?/g, "\n").split("\n")) {
    if (FENCE.test(line)) {
      if (inCode) {
        flush();
      } else {
        flush();
        current = { kind: "code", lines: [] };
      }
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      (current as { lines: string[] }).lines.push(line);
      continue;
    }
    if (line.trim() === "") {
      flush();
      continue;
    }
    const bullet = BULLET.exec(line);
    const numbered = NUMBERED.exec(line);
    const listKind = bullet !== null ? "ul" : numbered !== null ? "ol" : null;
    if (listKind !== null) {
      const text = (bullet ?? numbered)?.[1] ?? "";
      if (current?.kind === listKind) {
        current.items.push(text);
      } else {
        flush();
        current = { kind: listKind, items: [text] };
      }
      continue;
    }
    const heading = HEADING.exec(line);
    const text = heading === null ? line : `**${heading[1] ?? ""}**`;
    if (current?.kind === "paragraph") {
      current.lines.push(text);
    } else {
      flush();
      current = { kind: "paragraph", lines: [text] };
    }
  }
  flush();
  return blocks;
}

function renderBlock(block: Block): string {
  switch (block.kind) {
    case "paragraph":
      return `<p>${block.lines.map(renderInline).join("<br>")}</p>`;
    case "code":
      return `<pre><code>${escapeHtml(block.lines.join("\n"))}</code></pre>`;
    case "ul":
    case "ol": {
      const items = block.items
        .map((item) => `<li>${renderInline(item)}</li>`)
        .join("");
      return `<${block.kind}>${items}</${block.kind}>`;
    }
  }
}

/** HTML for `source`; safe to use as `innerHTML`. */
export function renderMarkdown(source: string): string {
  return parseBlocks(source).map(renderBlock).join("");
}
