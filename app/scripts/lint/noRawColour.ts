import colorNames from "color-name";
import type { Rule } from "eslint";

// An ESLint rule that fails on a colour written directly in TypeScript: a
// `#hex`, `rgb()`/`hsl()` (and the other colour functions), or a CSS named
// colour. Colours come from DESIGN.md through `src/design/tokens.ts` and the
// CSS variables, so a ticket can't add one by accident. Stylelint does the same
// for CSS (`stylelint.config.js`).
//
// The escape, for a colour that has to stay (with the reason after `--`):
//   // eslint-disable-next-line uta/no-raw-colour -- <reason>

const HEX = /(?:^|[^\w&])(#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4}))(?!\w)/i;
const FUNCTION = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/i;
const NAMED = new Set(Object.keys(colorNames));

/** The colour written in `text`, if there is one. */
export function rawColour(text: string): string | undefined {
  const hex = HEX.exec(text);
  if (hex) return hex[1];
  const fn = FUNCTION.exec(text);
  if (fn) return fn[0];
  const word = text.trim().toLowerCase();
  if (NAMED.has(word)) return word;
  return undefined;
}

const rule: Rule.RuleModule = {
  meta: {
    type: "problem",
    docs: { description: "Disallow colours written directly in the code; use a design token." },
    schema: [],
    messages: {
      raw:
        "`{{colour}}` is a colour written in the code. Use a token from DESIGN.md " +
        "(`src/design/tokens.ts` or a CSS variable). If it has to stay, add " +
        "`// eslint-disable-next-line uta/no-raw-colour -- <reason>`.",
    },
  },
  create(context) {
    const check = (node: Rule.Node, text: string) => {
      const colour = rawColour(text);
      if (colour) context.report({ node, messageId: "raw", data: { colour } });
    };
    return {
      Literal(node) {
        if (typeof node.value === "string") check(node, node.value);
      },
      TemplateElement(node) {
        check(node, node.value.raw);
      },
    };
  },
};

export default { rules: { "no-raw-colour": rule } };
