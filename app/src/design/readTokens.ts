import { spacing, typography } from "./tokens";

type Spacing = keyof typeof spacing;
type StrokeName = Extract<Spacing, `stroke${string}`>;
type TypeRole = keyof typeof typography;

export type LineWidths = Record<StrokeName, number>;
export type Fonts = Record<TypeRole, string>;

/**
 * Every line weight (`stroke-*`) as the page has it now, in CSS pixels, for
 * drawing on a canvas. Read from the CSS variables, like `readColours`, so
 * the tuning panel reaches the canvases. Without the stylesheet (as in tests)
 * each falls back to its value in DESIGN.md.
 */
export function readLineWidths(element: Element): LineWidths {
  const style = getComputedStyle(element);
  const names = (Object.keys(spacing) as Spacing[]).filter((name): name is StrokeName =>
    name.startsWith("stroke"),
  );
  return Object.fromEntries(
    names.map((name) => {
      const value = parseFloat(style.getPropertyValue(`--${kebab(name)}`));
      return [name, Number.isFinite(value) ? value : spacing[name]];
    }),
  ) as LineWidths;
}

/** Each type role as a canvas `font`, such as "400 11px system-ui", from the CSS variables. */
export function readFonts(element: Element): Fonts {
  const style = getComputedStyle(element);
  const roles = Object.keys(typography) as TypeRole[];
  return Object.fromEntries(
    roles.map((role) => {
      const read = (part: string) => style.getPropertyValue(`--font-${kebab(role)}-${part}`).trim();
      const [weight, size, family] = [read("weight"), read("size"), read("family")];
      return [role, weight && size && family ? `${weight} ${size} ${family}` : typography[role].font];
    }),
  ) as Fonts;
}

/** `strokeClipSelected` back to `stroke-clip-selected`, as DESIGN.md spells it. */
function kebab(name: string): string {
  return name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
}
