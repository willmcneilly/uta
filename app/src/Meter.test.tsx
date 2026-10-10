import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Meter } from "./Meter";
import { tokensChanged } from "./design/tokenChanges";
import { MeterLevel } from "./meterLevel";

/** A 2D context that records the colour of each rectangle it fills. */
function fakeContext() {
  const filled: string[] = [];
  const context = {
    fillStyle: "",
    scale: vi.fn(),
    fillRect: vi.fn(() => filled.push(context.fillStyle)),
  };
  return { context, filled };
}

describe("Meter", () => {
  let frames = new Map<number, FrameRequestCallback>();
  let nextFrame = 0;
  let schemeListeners: ((event: MediaQueryListEvent) => void)[] = [];
  const root = document.documentElement.style;

  beforeEach(() => {
    frames = new Map();
    schemeListeners = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frames.set(++nextFrame, callback);
      return nextFrame;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
    vi.stubGlobal("matchMedia", () => ({
      matches: false,
      addEventListener: (_: string, listener: (event: MediaQueryListEvent) => void) =>
        schemeListeners.push(listener),
      removeEventListener: () => {},
    }));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(null);
    document.documentElement.removeAttribute("style");
  });

  const runFrame = () => {
    const pending = [...frames.values()];
    frames.clear();
    for (const callback of pending) callback(performance.now());
  };

  it("draws in the colour tokens, and repaints in the new ones when the colour scheme changes", () => {
    const { context, filled } = fakeContext();
    vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(
      context as unknown as CanvasRenderingContext2D,
    );
    root.setProperty("--line-2", "light-track");
    root.setProperty("--signal", "light-signal");
    root.setProperty("--signal-hot", "light-hot");
    const level = new MeterLevel();
    render(<Meter level={level} label="Level meter" />);

    level.push(1); // 0 dB, the top of the scale
    runFrame();
    expect(filled).toEqual(["light-track", "light-signal", "light-hot"]);

    filled.length = 0;
    root.setProperty("--line-2", "dark-track");
    root.setProperty("--signal", "dark-signal");
    root.setProperty("--signal-hot", "dark-hot");
    act(() => schemeListeners.forEach((listener) => listener({ matches: true } as MediaQueryListEvent)));
    level.push(1);
    runFrame();
    expect(filled).toEqual(["dark-track", "dark-signal", "dark-hot"]);
  });

  it("repaints in the new colours when the tuning panel changes a token", () => {
    const { context, filled } = fakeContext();
    vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(
      context as unknown as CanvasRenderingContext2D,
    );
    root.setProperty("--signal", "before");
    const level = new MeterLevel();
    render(<Meter level={level} label="Level meter" />);
    level.push(0.01); // -40 dB, an ordinary level
    runFrame();
    expect(filled).toContain("before");

    filled.length = 0;
    root.setProperty("--signal", "after");
    act(() => tokensChanged());
    level.push(0.01);
    runFrame();
    expect(filled).toContain("after");
  });

  it("draws at the height it's given, the full height of its canvas", () => {
    const { context } = fakeContext();
    vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(
      context as unknown as CanvasRenderingContext2D,
    );
    const level = new MeterLevel();
    const { getByRole } = render(<Meter level={level} label="Track meter" width={148} height={4} />);
    const canvas = getByRole("img", { name: "Track meter" }) as HTMLCanvasElement;
    expect(canvas.style.height).toBe("4px");
    expect(canvas.height).toBe(4 * (window.devicePixelRatio || 1));
    act(() => {
      level.push(0.5);
      runFrame();
    });
    for (const [, , , height] of context.fillRect.mock.calls as unknown as number[][]) {
      expect(height).toBe(4);
    }
    expect(context.fillRect).toHaveBeenCalledWith(0, 0, 148, 4);
  });
});
