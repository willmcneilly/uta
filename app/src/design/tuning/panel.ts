// The tuning panel (RFC-005, part 3): live controls for DESIGN.md's tokens on
// the real app, in development builds only. main.tsx loads it only when
// `import.meta.env.DEV`, so a release build doesn't contain it. Each change
// regenerates the token CSS in the page and tells the canvases to read the
// tokens again; Save writes the values into DESIGN.md through the dev server.

import { type FolderApi, Pane } from "tweakpane";
import { parse } from "yaml";
import { type TokenEdits, applyEdits, frontMatterRange } from "../../../scripts/designEdits.ts";
import { DESIGN_ENDPOINT, type ServedDesign } from "../../../scripts/designEndpoints.ts";
import { readDesign, toCss } from "../../../scripts/designTokens.ts";
import { tokensChanged } from "../tokenChanges";

const DARK = "dark-";
const REFERENCE = /^\{.+\}$/;
const SYSTEM_FONTS = ["system-ui", "ui-monospace", "ui-serif", "ui-rounded"];
/** The spacing tokens the panel tunes: line weights, hatching and the millimetre grid. */
const TUNED_SPACING = /^(stroke-.+|hatch-gap|mm-grid-gap)$/;

type Theme = "system" | "light" | "dark";

/** A control's current value, as it's written in DESIGN.md. */
type Read = () => string | number;

export async function openTuningPanel(): Promise<void> {
  const response = await fetch(DESIGN_ENDPOINT);
  if (!response.ok) {
    console.warn(`The tuning panel couldn't read DESIGN.md: ${response.status}`);
    return;
  }
  const served = (await response.json()) as ServedDesign;
  void loadFonts(served);
  new TuningPanel(served);
}

class TuningPanel {
  /** DESIGN.md as last read or saved. */
  private base: string;
  private readonly controls = new Map<string, Read>();
  private readonly style = document.createElement("style");
  private readonly status = { message: "" };
  private readonly pane: Pane;

  constructor(served: ServedDesign) {
    this.base = served.markdown;
    this.style.dataset.utaTuning = "";
    document.head.append(this.style);

    const container = document.createElement("div");
    Object.assign(container.style, {
      position: "fixed",
      top: "8px",
      right: "8px",
      width: "300px",
      maxHeight: "calc(100vh - 16px)",
      overflowY: "auto",
      zIndex: "1000",
    });
    document.body.append(container);
    this.pane = new Pane({ container, title: "Design", expanded: false });

    const { start, end } = frontMatterRange(this.base);
    const front = parse(this.base.slice(start, end)) as FrontMatter;
    this.addTheme();
    this.addColours(front.colors, "Light", (name) => !name.startsWith(DARK));
    this.addColours(front.colors, "Dark", (name) => name.startsWith(DARK));
    this.addType(front.typography, served);
    this.addLines(front.spacing);

    this.pane.addButton({ title: "Save to DESIGN.md" }).on("click", () => void this.save());
    this.pane.addBinding(this.status, "message", {
      label: "",
      readonly: true,
      multiline: true,
      rows: 2,
    });
  }

  /** System, Light or Dark, for this session only: it isn't saved. */
  private addTheme(): void {
    const state: { theme: Theme } = { theme: "system" };
    this.pane
      .addBinding(state, "theme", {
        label: "theme",
        options: { System: "system", Light: "light", Dark: "dark" },
      })
      .on("change", ({ value }) => {
        if (value === "system") delete document.documentElement.dataset.theme;
        else document.documentElement.dataset.theme = value;
        tokensChanged();
      });
  }

  /** A colour picker for each colour that isn't a reference to another: those follow what they point at. */
  private addColours(colors: Record<string, string>, title: string, include: (name: string) => boolean): void {
    const folder = this.pane.addFolder({ title: `Colours: ${title}`, expanded: false });
    const state: Record<string, string> = {};
    for (const [name, value] of Object.entries(colors)) {
      if (!include(name) || name === "primary" || REFERENCE.test(value)) continue;
      state[name] = value;
      const label = name.startsWith(DARK) ? name.slice(DARK.length) : name;
      this.bind(folder, `colors.${name}`, state, name, {
        label,
        color: { alpha: value.length === 9 || value.length === 5 },
      });
    }
  }

  /** Each type role's family, size, weight and line height. */
  private addType(typography: Record<string, TypeToken>, served: ServedDesign): void {
    const families = [...SYSTEM_FONTS, ...new Set(served.fonts.map((font) => font.family))];
    for (const [role, token] of Object.entries(typography)) {
      const folder = this.pane.addFolder({ title: `Type: ${role}`, expanded: false });
      const state = {
        fontFamily: token.fontFamily,
        fontSize: parseFloat(token.fontSize),
        fontWeight: Number(token.fontWeight),
        lineHeight: token.lineHeight,
      };
      const options = Object.fromEntries(
        [...new Set([token.fontFamily, ...families])].map((family) => [family, family]),
      );
      const path = `typography.${role}`;
      this.bind(folder, `${path}.fontFamily`, state, "fontFamily", { label: "family", options });
      this.bind(folder, `${path}.fontSize`, state, "fontSize", { label: "size", min: 6, max: 40, step: 0.5 }, px);
      this.bind(folder, `${path}.fontWeight`, state, "fontWeight", {
        label: "weight",
        min: 100,
        max: 900,
        step: 100,
      });
      this.bind(folder, `${path}.lineHeight`, state, "lineHeight", {
        label: "line height",
        min: 0.8,
        max: 2,
        step: 0.05,
      });
    }
  }

  /** Line weights, hatch spacing and grid spacing. */
  private addLines(spacing: Record<string, string>): void {
    const folder = this.pane.addFolder({ title: "Lines, hatch and grid", expanded: false });
    const state: Record<string, number> = {};
    for (const [name, value] of Object.entries(spacing)) {
      if (!TUNED_SPACING.test(name)) continue;
      state[name] = parseFloat(value);
      const line = name.startsWith("stroke-");
      this.bind(
        folder,
        `spacing.${name}`,
        state,
        name,
        line ? { min: 0.25, max: 6, step: 0.25 } : { min: 1, max: 60, step: 1 },
        px,
      );
    }
  }

  /** A control for one token, at `path` in DESIGN.md's front matter. */
  private bind<T extends object>(
    folder: FolderApi,
    path: string,
    state: T,
    key: keyof T & string,
    params: Record<string, unknown>,
    write: (value: number) => string | number = (value) => value,
  ): void {
    this.controls.set(path, () => {
      const value = state[key];
      return typeof value === "number" ? write(value) : String(value);
    });
    folder.addBinding(state, key, params).on("change", () => this.preview());
  }

  private edits(): TokenEdits {
    return Object.fromEntries([...this.controls].map(([path, read]) => [path, read()]));
  }

  /** Shows the panel's values on the page: the CSS, and through `tokensChanged`, the canvases. */
  private preview(): void {
    try {
      this.style.textContent = toCss(readDesign(applyEdits(this.base, this.edits())));
      this.status.message = "Not saved.";
      tokensChanged();
    } catch (error) {
      this.status.message = (error as Error).message;
    }
  }

  private async save(): Promise<void> {
    this.status.message = "Saving…";
    try {
      const response = await fetch(DESIGN_ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ edits: this.edits() }),
      });
      const result = (await response.json()) as { markdown?: string; changed?: boolean; error?: string };
      if (!response.ok || result.markdown === undefined) {
        this.status.message = `Not saved: ${result.error ?? response.status}`;
        return;
      }
      this.base = result.markdown;
      this.status.message = result.changed ? "Saved to DESIGN.md." : "Nothing to save.";
    } catch (error) {
      this.status.message = `Not saved: ${(error as Error).message}`;
    }
  }
}

interface TypeToken {
  fontFamily: string;
  fontSize: string;
  fontWeight: number | string;
  lineHeight: number;
}

interface FrontMatter {
  colors: Record<string, string>;
  typography: Record<string, TypeToken>;
  spacing: Record<string, string>;
}

/** Sizes are written in pixels, rounded so a slider's step doesn't leave a long tail. */
function px(value: number): string {
  return `${Math.round(value * 100) / 100}px`;
}

/**
 * Makes the repo's and the trial folder's fonts usable by name, in the CSS
 * and on the canvases, then has the canvases draw again once they've loaded.
 */
async function loadFonts({ fonts }: ServedDesign): Promise<void> {
  const faces = fonts.map((font) => {
    const face = new FontFace(font.family, `url("${font.url}")`, {
      weight: font.weight,
      style: font.style,
    });
    document.fonts.add(face);
    return face.load();
  });
  if (faces.length === 0) return;
  await Promise.allSettled(faces);
  tokensChanged();
}
