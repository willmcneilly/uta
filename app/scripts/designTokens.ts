// Turns DESIGN.md's front matter into the app's token files: CSS variables for
// the stylesheets, and the same values, typed, for the canvases. The format
// (github.com/google-labs-code/design.md) has no themes, so a colour's dark
// twin is named with a `dark-` prefix. We read the YAML ourselves rather than
// use the format's exporter, so a change to the format only changes this file.

import { parse } from "yaml";

export class DesignTokenError extends Error {}

export interface TypeToken {
  fontFamily: string;
  /** In CSS pixels. */
  fontSize: number;
  fontWeight: number;
  /** A multiple of the font size. */
  lineHeight: number;
}

/** Every value resolved, with names as DESIGN.md spells them. */
export interface DesignTokens {
  colors: { light: Record<string, string>; dark: Record<string, string> };
  typography: Record<string, TypeToken>;
  /** In CSS pixels. */
  rounded: Record<string, number>;
  /** In CSS pixels, line weights (`stroke-*`) included. */
  spacing: Record<string, number>;
}

const DARK = "dark-";
/** Only there because the format requires it. It points at `ink`. */
const FORMAT_ONLY_COLORS = new Set(["primary"]);
const REFERENCE = /^\{([a-z0-9.-]+)\}$/i;
const MAX_REFERENCE_DEPTH = 10;
const NAME = /^[a-z][a-z0-9]*(-[a-z0-9]+)*$/;

/** Reads DESIGN.md's text, checks its tokens and resolves `{colors.ink}` references. */
export function readDesign(markdown: string): DesignTokens {
  const front = frontMatter(markdown);
  let data: unknown;
  try {
    data = parse(front);
  } catch (error) {
    throw new DesignTokenError(
      `DESIGN.md's front matter isn't valid YAML: ${(error as Error).message}`,
    );
  }
  if (!isRecord(data)) throw new DesignTokenError("DESIGN.md's front matter is empty.");
  const root = data;
  const resolve = (path: string) => resolveValue(root, path, root, path, 0);

  const colors = group(root, "colors");
  const light: Record<string, string> = {};
  const dark: Record<string, string> = {};
  for (const name of Object.keys(colors)) {
    if (FORMAT_ONLY_COLORS.has(name)) continue;
    checkName("colors", name);
    const value = resolve(`colors.${name}`);
    if (typeof value !== "string" || !isColor(value)) {
      throw new DesignTokenError(
        `colors.${name} should be a colour such as "#22262a", but it's ${show(value)}.`,
      );
    }
    if (name.startsWith(DARK)) dark[name.slice(DARK.length)] = value;
    else light[name] = value;
  }
  for (const name of Object.keys(light)) {
    if (!(name in dark)) {
      throw new DesignTokenError(
        `colors.${name} has no dark twin: add colors.${DARK}${name} for the dark theme.`,
      );
    }
  }
  for (const name of Object.keys(dark)) {
    if (!(name in light)) {
      throw new DesignTokenError(
        `colors.${DARK}${name} has no light twin: add colors.${name} for the light theme.`,
      );
    }
  }

  const typography: Record<string, TypeToken> = {};
  for (const [role, token] of Object.entries(group(root, "typography"))) {
    checkName("typography", role);
    const path = `typography.${role}`;
    if (!isRecord(token)) {
      throw new DesignTokenError(`${path} should have fontFamily, fontSize, fontWeight and lineHeight.`);
    }
    const property = (key: string) => {
      if (!(key in token)) throw new DesignTokenError(`${path} is missing ${key}.`);
      return resolve(`${path}.${key}`);
    };
    const fontFamily = property("fontFamily");
    if (typeof fontFamily !== "string" || fontFamily.trim() === "") {
      throw new DesignTokenError(`${path}.fontFamily should be a font name, but it's ${show(fontFamily)}.`);
    }
    const fontWeight = Number(property("fontWeight"));
    if (!Number.isInteger(fontWeight) || fontWeight < 1 || fontWeight > 1000) {
      throw new DesignTokenError(
        `${path}.fontWeight should be a number such as 400, but it's ${show(token.fontWeight)}.`,
      );
    }
    const lineHeight = property("lineHeight");
    if (typeof lineHeight !== "number" || !(lineHeight > 0)) {
      throw new DesignTokenError(
        `${path}.lineHeight should be a multiple of the font size, such as 1.2, but it's ${show(lineHeight)}.`,
      );
    }
    typography[role] = {
      fontFamily,
      fontSize: pixels(`${path}.fontSize`, property("fontSize")),
      fontWeight,
      lineHeight,
    };
  }

  const dimensions = (name: "rounded" | "spacing") => {
    const out: Record<string, number> = {};
    for (const key of Object.keys(group(root, name))) {
      checkName(name, key);
      out[key] = pixels(`${name}.${key}`, resolve(`${name}.${key}`));
    }
    return out;
  };

  const rounded = dimensions("rounded");
  const spacing = dimensions("spacing");
  const tokens = { colors: { light, dark }, typography, rounded, spacing };
  checkUnique(cssVariables(tokens).map(([name]) => name), "CSS variable");
  for (const [name, values] of Object.entries({ colors: light, typography, rounded, spacing })) {
    checkUnique(Object.keys(values).map(camel), `TypeScript name in ${name}`);
  }
  return tokens;
}

/** `tokens.css`: light colours in `:root`, dark ones for the system's dark mode, and both again under `data-theme`. */
export function toCss(tokens: DesignTokens): string {
  const block = (selector: string, lines: string[], indent = "") =>
    [`${indent}${selector} {`, ...lines.map((line) => `${indent}  ${line}`), `${indent}}`].join("\n");
  const colorLines = (theme: "light" | "dark") => [
    `color-scheme: ${theme};`,
    ...Object.entries(tokens.colors[theme]).map(([name, value]) => `--${name}: ${value};`),
  ];
  const rootLines = [
    "color-scheme: light dark;",
    ...cssVariables(tokens).map(([name, value]) => `${name}: ${value};`),
  ];
  return [
    HEADER_CSS,
    block(":root", rootLines),
    "",
    "@media (prefers-color-scheme: dark) {",
    block(":root", colorLines("dark"), "  "),
    "}",
    "",
    "/* Chooses a theme whatever the system's setting. Only development tools set it. */",
    block(':root[data-theme="light"]', colorLines("light")),
    "",
    block(':root[data-theme="dark"]', colorLines("dark")),
    "",
  ].join("\n");
}

/** `tokens.ts`: the same values for code, with names in camelCase (`ink-2` is `ink2`). */
export function toTs(tokens: DesignTokens): string {
  const object = (entries: [string, string][], indent: string) =>
    ["{", ...entries.map(([key, value]) => `${indent}  ${key}: ${value},`), `${indent}}`].join("\n");
  const colors = (theme: "light" | "dark") =>
    object(
      Object.entries(tokens.colors[theme]).map(([name, value]) => [camel(name), JSON.stringify(value)]),
      "  ",
    );
  const variables = object(
    Object.keys(tokens.colors.light).map((name) => [camel(name), JSON.stringify(`--${name}`)]),
    "",
  );
  const typography = object(
    Object.entries(tokens.typography).map(([role, type]) => [
      camel(role),
      object(
        [
          ["fontFamily", JSON.stringify(type.fontFamily)],
          ["fontSize", String(type.fontSize)],
          ["fontWeight", String(type.fontWeight)],
          ["lineHeight", String(type.lineHeight)],
          ["font", JSON.stringify(canvasFont(type))],
        ],
        "  ",
      ),
    ]),
    "",
  );
  const numbers = (values: Record<string, number>) =>
    object(
      Object.entries(values).map(([name, value]) => [camel(name), String(value)]),
      "",
    );
  return [
    HEADER_TS,
    "export interface TypeToken {",
    "  fontFamily: string;",
    "  /** In CSS pixels. */",
    "  fontSize: number;",
    "  fontWeight: number;",
    "  /** A multiple of the font size. */",
    "  lineHeight: number;",
    "  /** For a canvas's `font`, such as \"500 13.5px system-ui\". */",
    "  font: string;",
    "}",
    "",
    "/** Each theme's colours. The stylesheets use them through the variables in `colorVariables`. */",
    `export const colors = {\n  light: ${colors("light")},\n  dark: ${colors("dark")},\n} as const;`,
    "",
    "export type ColorName = keyof typeof colors.light;",
    "",
    "/** The CSS variable that holds each colour. */",
    `export const colorVariables: Record<ColorName, string> = ${variables};`,
    "",
    `export const typography = ${typography} as const satisfies Record<string, TypeToken>;`,
    "",
    "/** Corner radii, in CSS pixels. */",
    `export const rounded = ${numbers(tokens.rounded)} as const;`,
    "",
    "/** Spacing, fixed sizes and line weights (`stroke*`), in CSS pixels. */",
    `export const spacing = ${numbers(tokens.spacing)} as const;`,
    "",
  ].join("\n");
}

const GENERATED = "Generated from DESIGN.md by `npm run tokens`. Don't edit it: change DESIGN.md and run that again.";
const HEADER_CSS = `/* ${GENERATED} */\n`;
const HEADER_TS = `// ${GENERATED}\n`;

/** The variables that don't change with the theme, plus the light colours, in the order they're written. */
function cssVariables(tokens: DesignTokens): [string, string][] {
  const out: [string, string][] = Object.entries(tokens.colors.light).map(([name, value]) => [
    `--${name}`,
    value,
  ]);
  for (const [role, type] of Object.entries(tokens.typography)) {
    out.push(
      [`--font-${role}`, `${type.fontWeight} ${type.fontSize}px/${type.lineHeight} ${cssFamily(type.fontFamily)}`],
      [`--font-${role}-family`, cssFamily(type.fontFamily)],
      [`--font-${role}-size`, `${type.fontSize}px`],
      [`--font-${role}-weight`, String(type.fontWeight)],
      [`--font-${role}-line-height`, String(type.lineHeight)],
    );
  }
  for (const [name, value] of Object.entries(tokens.rounded)) out.push([`--rounded-${name}`, `${value}px`]);
  for (const [name, value] of Object.entries(tokens.spacing)) out.push([`--${name}`, `${value}px`]);
  return out;
}

function canvasFont(type: TypeToken): string {
  return `${type.fontWeight} ${type.fontSize}px ${cssFamily(type.fontFamily)}`;
}

const GENERIC_FAMILIES = new Set([
  "system-ui",
  "ui-sans-serif",
  "ui-serif",
  "ui-monospace",
  "ui-rounded",
  "sans-serif",
  "serif",
  "monospace",
]);

/** Quotes a family name such as Commit Mono, but not a keyword such as system-ui or a list. */
function cssFamily(family: string): string {
  if (GENERIC_FAMILIES.has(family) || family.includes(",") || /^["']/.test(family)) return family;
  return JSON.stringify(family);
}

function frontMatter(markdown: string): string {
  const lines = markdown.split(/\r?\n/);
  if (lines[0] !== "---") {
    throw new DesignTokenError("DESIGN.md has no front matter: its first line should be ---.");
  }
  const end = lines.indexOf("---", 1);
  if (end < 0) throw new DesignTokenError("DESIGN.md's front matter has no closing --- line.");
  return lines.slice(1, end).join("\n");
}

function group(root: Record<string, unknown>, name: string): Record<string, unknown> {
  const value = root[name];
  if (!isRecord(value)) throw new DesignTokenError(`DESIGN.md's front matter has no ${name} group.`);
  return value;
}

/** The value at `path`, following references to other tokens. */
function resolveValue(
  root: Record<string, unknown>,
  path: string,
  from: Record<string, unknown>,
  original: string,
  depth: number,
): unknown {
  if (depth > MAX_REFERENCE_DEPTH) {
    throw new DesignTokenError(`${original} refers to itself through a loop of references.`);
  }
  let value: unknown = from;
  for (const key of path.split(".")) {
    value = isRecord(value) && Object.hasOwn(value, key) ? value[key] : undefined;
  }
  const reference = typeof value === "string" ? REFERENCE.exec(value) : null;
  if (!reference) return value;
  const target = reference[1];
  let exists: unknown = root;
  for (const key of target.split(".")) exists = isRecord(exists) ? exists[key] : undefined;
  if (exists === undefined) {
    throw new DesignTokenError(`${original} refers to {${target}}, which doesn't exist.`);
  }
  return resolveValue(root, target, root, original, depth + 1);
}

function pixels(path: string, value: unknown): number {
  const match = typeof value === "string" ? /^(\d+(?:\.\d+)?)px$/.exec(value) : null;
  if (!match) {
    throw new DesignTokenError(`${path} should be a size in pixels, such as "24px", but it's ${show(value)}.`);
  }
  return Number(match[1]);
}

const HEX = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const FUNCTION = /^(rgba?|hsla?|hwb|oklch|oklab|lch|lab|color-mix)\(.+\)$/i;

function isColor(value: string): boolean {
  return HEX.test(value) || FUNCTION.test(value) || /^[a-z]+$/i.test(value);
}

function checkName(groupName: string, name: string): void {
  if (!NAME.test(name)) {
    throw new DesignTokenError(
      `${groupName}.${name} isn't a usable name: use lowercase words joined by hyphens, such as ink-2.`,
    );
  }
}

function checkUnique(names: string[], what: string): void {
  const seen = new Set<string>();
  for (const name of names) {
    if (seen.has(name)) throw new DesignTokenError(`Two tokens give the same ${what}, ${name}: rename one.`);
    seen.add(name);
  }
}

function camel(name: string): string {
  return name.replace(/-([a-z0-9])/g, (_, next: string) => next.toUpperCase());
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function show(value: unknown): string {
  return value === undefined ? "missing" : JSON.stringify(value);
}
