import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { createCanvas2DRenderer } from "../pianoRoll/canvasRenderer";
import type { Layers } from "../pianoRoll/renderer";
import { Updates, projectView } from "../pianoRoll/testing";
import { createTimelineRenderer } from "../timeline/canvasRenderer";
import type { TimelineLayers } from "../timeline/renderer";
import { tokensChanged } from "./tokenChanges";

// A change from the tuning panel reaches the real timeline and piano roll
// renderers: they draw with the new line weights and fonts.

/** A 2D context that records the fonts it's given and the rectangles it fills. */
class RecordingContext {
  fonts: string[] = [];
  widths: number[] = [];

  constructor() {
    return new Proxy(this, {
      get: (target, key) =>
        key in target ? target[key as keyof RecordingContext] : key === "fillRect" ? target.fillRect : () => {},
      set: (target, key, value) => {
        if (key === "font") target.fonts.push(value as string);
        return true;
      },
    });
  }

  fillRect = (_x: number, _y: number, width: number) => {
    this.widths.push(width);
  };
}

/** Gives each of a renderer's canvases a recording context. */
function record(canvases: HTMLCanvasElement[], into: RecordingContext[]) {
  for (const canvas of canvases) {
    const context = new RecordingContext();
    into.push(context);
    canvas.getContext = (() => context) as unknown as HTMLCanvasElement["getContext"];
  }
}

class FixedSizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe() {
    const entry = { contentRect: { width: 856, height: 504 } } as ResizeObserverEntry;
    this.callback([entry], this as unknown as ResizeObserver);
  }
  unobserve() {}
  disconnect() {}
}

describe("a token change", () => {
  let roll: RecordingContext[];
  let timeline: RecordingContext[];

  beforeEach(() => {
    roll = [];
    timeline = [];
    vi.stubGlobal("ResizeObserver", FixedSizeObserver);
    const project = projectView();
    const updates = new Updates();
    mockIPC(
      (cmd, payload) => {
        const args = (payload ?? {}) as Record<string, unknown>;
        if (cmd === "get_project") return updates.send(project);
        if (cmd === "get_notes") return updates.notes(project, args.clip as string);
        return null;
      },
      { shouldMockEvents: true },
    );
  });

  afterEach(async () => {
    cleanup();
    // Let the app stop listening for events before the mocks go.
    await new Promise((resolve) => setTimeout(resolve, 0));
    clearMocks();
    vi.unstubAllGlobals();
    document.documentElement.removeAttribute("style");
  });

  const setLabel = (size: string) => {
    const root = document.documentElement.style;
    root.setProperty("--font-label-weight", "400");
    root.setProperty("--font-label-size", size);
    root.setProperty("--font-label-family", "system-ui");
  };

  const latest = (contexts: RecordingContext[]) => contexts.slice(-3);
  const fonts = (contexts: RecordingContext[]) => latest(contexts).flatMap((c) => c.fonts);
  const widths = (contexts: RecordingContext[]) => latest(contexts).flatMap((c) => c.widths);

  it("reaches the timeline and piano roll renderers", async () => {
    setLabel("17px");
    document.documentElement.style.setProperty("--stroke-grid", "3px");
    render(
      <App
        createRenderer={(layers: Layers) => {
          record([layers.grid, layers.notes, layers.top], roll);
          return createCanvas2DRenderer(layers);
        }}
        createTimelineRenderer={(layers: TimelineLayers) => {
          record([layers.grid, layers.clips, layers.top], timeline);
          return createTimelineRenderer(layers);
        }}
      />,
    );
    await screen.findByRole("application", { name: "Notes" });
    await waitFor(() => {
      expect(fonts(roll)).toContain("400 17px system-ui");
      expect(fonts(timeline)).toContain("400 17px system-ui");
    });
    expect(widths(roll)).toContain(3);
    expect(widths(timeline)).toContain(3);

    // The panel changes the tokens, then says so.
    setLabel("19px");
    document.documentElement.style.setProperty("--stroke-grid", "2.5px");
    act(() => tokensChanged());
    await waitFor(() => {
      expect(fonts(roll)).toContain("400 19px system-ui");
      expect(fonts(timeline)).toContain("400 19px system-ui");
    });
    expect(fonts(roll)).not.toContain("400 17px system-ui");
    expect(widths(roll)).toContain(2.5);
    expect(widths(timeline)).toContain(2.5);
  });
});
