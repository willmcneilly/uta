// The dev server's paths for the tuning panel, shared by the server and the
// panel. Nothing in a release build refers to them.

/** Where the panel reads DESIGN.md and its fonts (GET) and saves to it (POST). */
export const DESIGN_ENDPOINT = "/__uta/design";
/** Where the panel loads the fonts it offers, as `/__uta/fonts/<source>/<file>`. */
export const FONTS_ENDPOINT = "/__uta/fonts";

/** What a GET of the design endpoint answers. */
export interface ServedDesign {
  markdown: string;
  fonts: ServedFont[];
}

export interface ServedFont {
  family: string;
  /** A CSS font-weight, or a range such as "100 900" for a variable font. */
  weight: string;
  style: "normal" | "italic";
  url: string;
}
