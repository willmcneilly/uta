// Just enough TOML for the listening sessions (session.toml and
// verdict.toml), since the scripts use Node's built-ins only. It reads and
// writes strings (basic, literal and multi-line), numbers, booleans, arrays
// of those, tables and arrays of tables. Inline tables and dates aren't
// needed, so they're an error rather than a guess.

export type TomlValue = string | number | boolean | TomlValue[] | TomlTable;
export interface TomlTable {
  [key: string]: TomlValue;
}

export class TomlError extends Error {
  constructor(message: string, line: number) {
    super(`line ${line}: ${message}`);
  }
}

/** Reads a TOML document into plain objects. */
export function parseToml(text: string): TomlTable {
  const root: TomlTable = {};
  let table = root;
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  for (let index = 0; index < lines.length; index++) {
    const lineNumber = index + 1;
    const line = stripComment(lines[index]).trim();
    if (line === "") continue;
    const header = /^(\[\[?)\s*([^\]]+?)\s*(\]\]?)$/.exec(line);
    if (header) {
      const [, open, name, close] = header;
      if (open.length !== close.length) throw new TomlError(`a table header needs matching brackets: ${line}`, lineNumber);
      table = open === "[[" ? appendTable(root, keyPath(name, lineNumber), lineNumber) : openTable(root, keyPath(name, lineNumber), lineNumber);
      continue;
    }
    const equals = findEquals(line);
    if (equals < 0) throw new TomlError(`expected \`key = value\`: ${line}`, lineNumber);
    const path = keyPath(line.slice(0, equals), lineNumber);
    let rest = line.slice(equals + 1).trim();
    // A multi-line string or array goes on until it closes.
    while (unclosed(rest)) {
      index++;
      if (index >= lines.length) throw new TomlError("this value never closes", lineNumber);
      rest += "\n" + (rest.startsWith('"""') || rest.startsWith("'''") ? lines[index] : stripComment(lines[index]));
    }
    const reader = new ValueReader(rest, lineNumber);
    const value = reader.value();
    reader.end();
    const parent = openTable(table, path.slice(0, -1), lineNumber);
    const last = path[path.length - 1];
    if (last in parent) throw new TomlError(`\`${path.join(".")}\` is set twice`, lineNumber);
    parent[last] = value;
  }
  return root;
}

function stripComment(line: string): string {
  let quote: string | null = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (quote) {
      if (c === "\\" && quote === '"') i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") quote = c;
    else if (c === "#") return line.slice(0, i);
  }
  return line;
}

function findEquals(line: string): number {
  let quote: string | null = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === '"' || c === "'") quote = c;
    else if (c === "=") return i;
  }
  return -1;
}

function unclosed(value: string): boolean {
  for (const fence of ['"""', "'''"]) {
    if (value.startsWith(fence)) return value.indexOf(fence, 3) < 0;
  }
  if (!value.startsWith("[")) return false;
  let depth = 0;
  let quote: string | null = null;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (quote) {
      if (c === "\\" && quote === '"') i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") quote = c;
    else if (c === "[") depth++;
    else if (c === "]") depth--;
  }
  return depth > 0;
}

function keyPath(text: string, line: number): string[] {
  return text.split(".").map((part) => {
    const key = part.trim();
    const quoted = /^"([^"]*)"$|^'([^']*)'$/.exec(key);
    if (quoted) return quoted[1] ?? quoted[2];
    if (!/^[A-Za-z0-9_-]+$/.test(key)) throw new TomlError(`\`${text.trim()}\` isn't a key`, line);
    return key;
  });
}

function openTable(root: TomlTable, path: string[], line: number): TomlTable {
  let table = root;
  for (const key of path) {
    const next = table[key];
    if (next === undefined) table = table[key] = {};
    else if (Array.isArray(next) && isTable(next[next.length - 1])) table = next[next.length - 1] as TomlTable;
    else if (isTable(next)) table = next;
    else throw new TomlError(`\`${key}\` is already a value, not a table`, line);
  }
  return table;
}

function appendTable(root: TomlTable, path: string[], line: number): TomlTable {
  const parent = openTable(root, path.slice(0, -1), line);
  const key = path[path.length - 1];
  const list = parent[key] ?? (parent[key] = []);
  if (!Array.isArray(list)) throw new TomlError(`\`${key}\` is already a value, not a list of tables`, line);
  const table: TomlTable = {};
  list.push(table);
  return table;
}

function isTable(value: TomlValue | undefined): value is TomlTable {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

class ValueReader {
  private at = 0;
  private readonly text: string;
  private readonly line: number;

  constructor(text: string, line: number) {
    this.text = text;
    this.line = line;
  }

  value(): TomlValue {
    this.space();
    const rest = this.text.slice(this.at);
    if (rest.startsWith('"""')) return this.multiline('"""');
    if (rest.startsWith("'''")) return this.multiline("'''");
    const c = rest[0];
    if (c === '"') return this.basic();
    if (c === "'") return this.literal();
    if (c === "[") return this.array();
    if (c === "{") throw new TomlError("inline tables aren't supported here: use a [table]", this.line);
    const word = /^[^\s,\]]+/.exec(rest)?.[0] ?? "";
    this.at += word.length;
    if (word === "true") return true;
    if (word === "false") return false;
    const number = word.replace(/_/g, "");
    if (/^[+-]?(\d+(\.\d+)?([eE][+-]?\d+)?|inf|nan)$/.test(number)) {
      const n = number.replace(/^([+-]?)inf$/, "$1Infinity").replace(/^[+-]?nan$/, "NaN");
      return Number(n);
    }
    throw new TomlError(`\`${word || rest}\` isn't a value (strings need quotes)`, this.line);
  }

  end(): void {
    this.space();
    if (this.at < this.text.length) throw new TomlError(`unexpected \`${this.text.slice(this.at)}\` after the value`, this.line);
  }

  private space(): void {
    while (this.at < this.text.length && /\s/.test(this.text[this.at])) this.at++;
  }

  private basic(): string {
    this.at++;
    let out = "";
    while (this.at < this.text.length) {
      const c = this.text[this.at++];
      if (c === '"') return out;
      if (c === "\\") {
        const [escaped, next] = readEscape(this.text, this.at, this.line);
        out += escaped;
        this.at = next;
      }
      else if (c === "\n") break;
      else out += c;
    }
    throw new TomlError("a string never closes", this.line);
  }

  private literal(): string {
    const end = this.text.indexOf("'", this.at + 1);
    if (end < 0) throw new TomlError("a string never closes", this.line);
    const out = this.text.slice(this.at + 1, end);
    this.at = end + 1;
    return out;
  }

  private multiline(fence: string): string {
    const end = this.text.indexOf(fence, this.at + 3);
    let body = this.text.slice(this.at + 3, end);
    this.at = end + 3;
    if (body.startsWith("\n")) body = body.slice(1);
    if (fence === "'''") return body;
    let out = "";
    for (let i = 0; i < body.length; i++) {
      if (body[i] !== "\\") {
        out += body[i];
        continue;
      }
      // A backslash at the end of a line joins it to the next.
      const join = /^\\[ \t]*\n\s*/.exec(body.slice(i));
      if (join) {
        i += join[0].length - 1;
        continue;
      }
      const [escaped, next] = readEscape(body, i + 1, this.line);
      out += escaped;
      i = next - 1;
    }
    return out;
  }

  private array(): TomlValue[] {
    this.at++;
    const out: TomlValue[] = [];
    for (;;) {
      this.space();
      if (this.text[this.at] === "]") {
        this.at++;
        return out;
      }
      out.push(this.value());
      this.space();
      const c = this.text[this.at];
      if (c === ",") this.at++;
      else if (c !== "]") throw new TomlError("expected `,` or `]` in a list", this.line);
    }
  }
}

const ESCAPES: Record<string, string> = { b: "\b", t: "\t", n: "\n", f: "\f", r: "\r", '"': '"', "\\": "\\" };

/** The character an escape stands for, from just after its backslash, and where it ends. */
function readEscape(text: string, at: number, line: number): [string, number] {
  const c = text[at];
  if (c in ESCAPES) return [ESCAPES[c], at + 1];
  if (c === "u" || c === "U") {
    const length = c === "u" ? 4 : 8;
    const hex = text.slice(at + 1, at + 1 + length);
    if (!new RegExp(`^[0-9a-fA-F]{${length}}$`).test(hex)) throw new TomlError(`\`\\${c}${hex}\` isn't an escape`, line);
    return [String.fromCodePoint(parseInt(hex, 16)), at + 1 + length];
  }
  throw new TomlError(`\`\\${c}\` isn't an escape`, line);
}

/**
 * Writes plain objects as TOML: values first, then each table under its
 * header, in the order they were given, so a file reads the way it was built.
 */
export function stringifyToml(table: TomlTable): string {
  const out: string[] = [];
  writeTable(table, [], out);
  return out.join("\n").replace(/^\n+/, "") + "\n";
}

function writeTable(table: TomlTable, path: string[], out: string[]): void {
  const entries = Object.entries(table).filter(([, value]) => value !== undefined);
  const nested = (value: TomlValue) => isTable(value) || (Array.isArray(value) && value.length > 0 && value.every(isTable));
  for (const [key, value] of entries) {
    if (!nested(value)) out.push(`${writeKey(key)} = ${writeValue(value)}`);
  }
  for (const [key, value] of entries) {
    if (!nested(value)) continue;
    const name = [...path, key].map(writeKey).join(".");
    if (isTable(value)) {
      out.push("", `[${name}]`);
      writeTable(value, [...path, key], out);
    } else {
      for (const item of value as TomlTable[]) {
        out.push("", `[[${name}]]`);
        writeTable(item, [...path, key], out);
      }
    }
  }
}

function writeKey(key: string): string {
  return /^[A-Za-z0-9_-]+$/.test(key) ? key : JSON.stringify(key);
}

function writeValue(value: TomlValue): string {
  if (typeof value === "string") return JSON.stringify(value).replace(/[\u007f-\u009f]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`);
  if (typeof value === "boolean") return String(value);
  if (typeof value === "number") {
    if (Number.isNaN(value)) return "nan";
    if (!Number.isFinite(value)) return value > 0 ? "inf" : "-inf";
    return String(value);
  }
  if (Array.isArray(value)) return `[${value.map(writeValue).join(", ")}]`;
  throw new Error("a table inside a list can't be written inline");
}
