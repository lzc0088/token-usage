// Strip common markdown syntax from a release-note body so settings windows
// can render it as plain text (`white-space: pre-wrap` keeps the line
// breaks). Pure function, no dependencies — covers the constructs that
// appear in Gitee/GitHub release bodies: ATX headings, emphasis, inline
// code, fenced code, links, images, list markers and horizontal rules.

/**
 * Convert a markdown release body to readable plain text.
 * - `#`-headings lose their markers (text kept on its own line)
 * - `**bold**` / `*em*` / `` `code` `` unwrap to their inner text
 * - `[text](url)` → `text`; `![alt](url)` removed entirely
 * - `-` / `*` / `+` list markers become `•`
 * - fenced code blocks lose their backtick fences (content kept)
 * - `---` horizontal rules are dropped
 * - runs of blank lines collapse to one; ends trimmed
 */
export function markdownToPlainText(md: string): string {
  return md
    .split("\n")
    .map((line) => stripInline(line))
    .filter((line) => !/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line))
    .join("\n")
    // Blank lines carry no meaning in a compact changelog box — drop them
    // all so the text stays tight inside the scrollable area.
    .replace(/\n{2,}/g, "\n")
    .trim();
}

/** Strip markdown from a single line (also handles fenced-code markers). */
function stripInline(line: string): string {
  let out = line;

  // Fenced code markers → drop (content of the block is kept as plain lines).
  out = out.replace(/^\s*```\S*\s*$/, "");

  // ATX headings: leading #'s (keep the text, trim).
  out = out.replace(/^\s{0,3}#{1,6}\s+(.*)$/, "$1");

  // Images removed entirely; links keep their text.
  out = out.replace(/!\[([^\]]*)\]\([^)]*\)/g, "");
  out = out.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");

  // Inline code and emphasis (bold before italic; multi-symbol first).
  out = out.replace(/`([^`]+)`/g, "$1");
  out = out.replace(/\*\*([^*]+)\*\*/g, "$1");
  out = out.replace(/__([^_]+)__/g, "$1");
  out = out.replace(/\*([^*]+)\*/g, "$1");
  out = out.replace(/(^|\W)_([^_]+)_(?=\W|$)/g, "$1$2");

  // Unordered list markers → bullet.
  out = out.replace(/^(\s*)[-*+]\s+/, "$1• ");

  return out;
}
