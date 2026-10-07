import { type ColorName, colorVariables, colors } from "./tokens";

export type Colours = Record<ColorName, string>;

/**
 * Every colour token as the page has it now, for drawing on a canvas. Read
 * from the CSS variables, so it follows the theme. Without the stylesheet
 * (as in tests) each falls back to its light value.
 */
export function readColours(element: Element): Colours {
  const style = getComputedStyle(element);
  const names = Object.keys(colorVariables) as ColorName[];
  return Object.fromEntries(
    names.map((name) => [
      name,
      style.getPropertyValue(colorVariables[name]).trim() || colors.light[name],
    ]),
  ) as Colours;
}
