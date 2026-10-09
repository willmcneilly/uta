// Stylelint fails on a colour written directly in CSS: colours come from
// DESIGN.md through the CSS variables in `src/design/tokens.css` (generated,
// so it's the one file allowed raw colours). ESLint does the same for
// TypeScript (`scripts/lint/noRawColour.ts`).
//
// The escape, for a colour that has to stay (with the reason after `--`):
//   /* stylelint-disable-next-line <rule> -- <reason> */

const message = (what) =>
  `${what} is a colour written in the code. Use a token from DESIGN.md (a CSS variable such as var(--ink)). ` +
  "If it has to stay, add /* stylelint-disable-next-line <rule> -- <reason> */.";

export default {
  // Every escape has to say why.
  reportDescriptionlessDisables: true,
  ignoreFiles: ["src/design/tokens.css", "dist/**", "src-tauri/**", "node_modules/**"],
  rules: {
    "color-no-hex": [true, { message: (hex) => message(hex) }],
    "color-named": ["never", { message: (name) => message(name) }],
    // CSS function names ignore case, so match them that way.
    "function-disallowed-list": [
      ["/^(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)$/i"],
      { message: (fn) => message(`${fn}()`) },
    ],
  },
};
