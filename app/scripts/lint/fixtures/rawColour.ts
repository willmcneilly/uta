// A colour written directly, for noRawColour.test.ts: the colour check must catch each.
export const hex = "#22262a";
export const fn = "rgb(34, 38, 42)";
export const hsl = "hsl(210deg 10% 15%)";
export const named = "red";
export const template = (alpha: number) => `rgba(0, 0, 0, ${alpha})`;

// eslint-disable-next-line uta/no-raw-colour -- the escape lets this one through
export const escaped = "#ffffff";

export const token = "var(--ink)";
export const words = "Track #1 is red";
export const uppercase = "RGB(34, 38, 42)";
export const colorFunction = "color(display-p3 0.1 0.2 0.3)";

// eslint-disable-next-line uta/no-raw-colour
export const reasonless = "#ffffff";
