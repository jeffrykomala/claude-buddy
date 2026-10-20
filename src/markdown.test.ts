import { describe, expect, it } from "vitest";
import { escapeHtml, renderMarkdown } from "./markdown";

describe("escapeHtml", () => {
  it("escapes the characters that matter in HTML", () => {
    expect(escapeHtml(`<a href="x">&'`)).toBe(
      "&lt;a href=&quot;x&quot;&gt;&amp;&#39;",
    );
  });
});

describe("renderMarkdown", () => {
  it("wraps text in a paragraph", () => {
    expect(renderMarkdown("Hello")).toBe("<p>Hello</p>");
  });

  it("splits paragraphs on blank lines and keeps single line breaks", () => {
    expect(renderMarkdown("a\nb\n\nc")).toBe("<p>a<br>b</p><p>c</p>");
  });

  it("renders bold and italic", () => {
    expect(renderMarkdown("**bold** and *italic*")).toBe(
      "<p><strong>bold</strong> and <em>italic</em></p>",
    );
  });

  it("renders inline code and keeps its contents literal", () => {
    expect(renderMarkdown("run `a **b** <c>` now")).toBe(
      "<p>run <code>a **b** &lt;c&gt;</code> now</p>",
    );
  });

  it("leaves an unfinished backtick alone (text is still streaming)", () => {
    expect(renderMarkdown("run `npm")).toBe("<p>run `npm</p>");
  });

  it("leaves unfinished bold alone", () => {
    expect(renderMarkdown("this is **bold")).toBe("<p>this is **bold</p>");
  });

  it("renders bullet lists", () => {
    expect(renderMarkdown("- one\n- two\n* three")).toBe(
      "<ul><li>one</li><li>two</li><li>three</li></ul>",
    );
  });

  it("renders numbered lists", () => {
    expect(renderMarkdown("1. a\n2) b")).toBe("<ol><li>a</li><li>b</li></ol>");
  });

  it("starts a new list when the kind changes", () => {
    expect(renderMarkdown("- a\n1. b")).toBe(
      "<ul><li>a</li></ul><ol><li>b</li></ol>",
    );
  });

  it("renders fenced code blocks without interpreting their contents", () => {
    const html = renderMarkdown("```ts\nconst a = 1 < 2;\n- not a list\n```");

    expect(html).toBe(
      "<pre><code>const a = 1 &lt; 2;\n- not a list</code></pre>",
    );
  });

  it("closes an unfinished code block", () => {
    expect(renderMarkdown("```\nabc")).toBe("<pre><code>abc</code></pre>");
  });

  it("shows headings as bold text", () => {
    expect(renderMarkdown("## Title")).toBe(
      "<p><strong>Title</strong></p>",
    );
  });

  it("never lets HTML through", () => {
    const html = renderMarkdown(
      '<script>alert(1)</script> <img src=x onerror="a()">',
    );

    expect(html).not.toContain("<script");
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;script&gt;");
  });

  it("leaves links as plain text", () => {
    expect(renderMarkdown("[hi](javascript:alert(1))")).toBe(
      "<p>[hi](javascript:alert(1))</p>",
    );
  });

  it("handles Windows line endings", () => {
    expect(renderMarkdown("a\r\nb")).toBe("<p>a<br>b</p>");
  });

  it("renders nothing for empty text", () => {
    expect(renderMarkdown("")).toBe("");
    expect(renderMarkdown("  \n\n ")).toBe("");
  });
});
