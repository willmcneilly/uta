// Writes token values back into DESIGN.md for the tuning panel. Only the
// front matter changes, and in it only the values that changed: each one is
// replaced where it sits in the text, so the prose, the comments, the order
// and the quoting all stay exactly as they were.

import { isScalar, parseDocument } from "yaml";
import { DesignTokenError } from "./designTokens.ts";

/** New values by path, such as `colors.ink` or `typography.name.fontSize`. */
export type TokenEdits = Record<string, string | number>;

/** `markdown` with each edited token's value replaced. Unchanged values are left as they're written. */
export function applyEdits(markdown: string, edits: TokenEdits): string {
  const { start, end } = frontMatterRange(markdown);
  const front = markdown.slice(start, end);
  const document = parseDocument(front);
  if (document.errors.length > 0) {
    throw new DesignTokenError(`DESIGN.md's front matter isn't valid YAML: ${document.errors[0].message}`);
  }

  const replacements: { from: number; to: number; text: string }[] = [];
  for (const [path, value] of Object.entries(edits)) {
    const node = document.getIn(path.split("."), true);
    if (!isScalar(node) || !node.range) {
      throw new DesignTokenError(`${path} isn't a token in DESIGN.md, so it can't be saved.`);
    }
    if (typeof value !== "string" && typeof value !== "number") {
      throw new DesignTokenError(`${path} should be a string or a number.`);
    }
    if (node.value === value) continue;
    replacements.push({ from: node.range[0], to: node.range[1], text: scalar(value, node.type) });
  }

  let out = front;
  for (const { from, to, text } of replacements.sort((a, b) => b.from - a.from)) {
    out = out.slice(0, from) + text + out.slice(to);
  }
  return markdown.slice(0, start) + out + markdown.slice(end);
}

/** Where the front matter's text starts and ends, between the `---` lines. */
export function frontMatterRange(markdown: string): { start: number; end: number } {
  const open = /^---\r?\n/.exec(markdown);
  if (!open) throw new DesignTokenError("DESIGN.md has no front matter: its first line should be ---.");
  const start = open[0].length;
  const close = /(^|\r?\n)---(\r?\n|$)/g;
  close.lastIndex = start - 1;
  for (let match = close.exec(markdown); match; match = close.exec(markdown)) {
    if (match.index + match[1].length >= start) {
      return { start, end: match.index + match[1].length };
    }
  }
  throw new DesignTokenError("DESIGN.md's front matter has no closing --- line.");
}

/** A value written in the same style as the one it replaces. */
function scalar(value: string | number, style: string | undefined): string {
  if (typeof value === "number") return String(value);
  if (style === "QUOTE_SINGLE") return `'${value.replaceAll("'", "''")}'`;
  if (style === "PLAIN" && isPlain(value)) return value;
  return JSON.stringify(value);
}

/** Whether `value` reads back as the same string when written without quotes. */
function isPlain(value: string): boolean {
  if (value === "" || /[\r\n#:]/.test(value) || value !== value.trim()) return false;
  try {
    return parseDocument(value).toJS() === value;
  } catch {
    return false;
  }
}
