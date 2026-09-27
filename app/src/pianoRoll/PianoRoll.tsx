import { useEffect, useRef, useState } from "react";
import type { ProjectView } from "../backend";
import { createCanvas2DRenderer } from "./canvasRenderer";
import type { FrameStats } from "./frameStats";
import type { PlayheadClock } from "./playhead";
import type { RendererFactory } from "./renderer";
import { PianoRollScene } from "./scene";
import { noteArea, zoomPitch, zoomTime } from "./viewport";

interface Props {
  project: ProjectView;
  /** Where the playhead is, between the engine's reports. */
  clock: PlayheadClock;
  /** Where each frame's timing goes. */
  stats: FrameStats;
  /** Draws the layers. Canvas 2D unless a test or WebGL says otherwise. */
  createRenderer?: RendererFactory;
}

/** Each zoom button press zooms by this much. */
const ZOOM_STEP = 1.25;
/** How much a wheel's `deltaY` zooms with ⌘, Ctrl (a trackpad pinch) or ⌥ held. */
const WHEEL_ZOOM_RATE = 0.01;

/**
 * The piano roll: the clip's notes over a keyboard and a bar ruler, with the
 * playhead. It's drawn on three stacked canvases, each redrawn only when it
 * needs to be (RFC-002, "The shared model", point 8).
 */
export function PianoRoll({
  project,
  clock,
  stats,
  createRenderer = createCanvas2DRenderer,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLCanvasElement>(null);
  const notesRef = useRef<HTMLCanvasElement>(null);
  const topRef = useRef<HTMLCanvasElement>(null);
  const [scene] = useState(() => new PianoRollScene());
  const scheme = useColourScheme();

  useEffect(() => scene.setProject(project), [scene, project]);

  // The size, as the window changes.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const resize = (width: number, height: number) =>
      scene.resize(width, height, window.devicePixelRatio || 1);
    if (typeof ResizeObserver === "undefined") {
      const box = container.getBoundingClientRect();
      resize(box.width, box.height);
      return;
    }
    const observer = new ResizeObserver(([entry]) =>
      resize(entry.contentRect.width, entry.contentRect.height),
    );
    observer.observe(container);
    return () => observer.disconnect();
  }, [scene]);

  // The renderer, and the drawing loop, once a screen frame. A new colour
  // scheme makes a new renderer, which reads the new colours.
  useEffect(() => {
    const grid = gridRef.current;
    const notes = notesRef.current;
    const top = topRef.current;
    if (!grid || !notes || !top) return;
    scene.setRenderer(createRenderer({ grid, notes, top }));

    let request = 0;
    const frame = (now: number) => {
      const started = performance.now();
      scene.draw(clock.at(now));
      stats.record(now, performance.now() - started);
      request = requestAnimationFrame(frame);
    };
    request = requestAnimationFrame(frame);
    // Frames stop while the window is hidden; that gap isn't a slow frame.
    const onVisibility = () => stats.pause();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelAnimationFrame(request);
      document.removeEventListener("visibilitychange", onVisibility);
      scene.setRenderer(null);
    };
  }, [scene, clock, stats, createRenderer, scheme]);

  // Scroll with the wheel or trackpad; zoom time with ⌘ or a pinch, and
  // pitch with ⌥. Not a React handler: it has to be able to preventDefault.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = container.getBoundingClientRect();
      const x = event.clientX - box.left;
      const y = event.clientY - box.top;
      const zoom = Math.exp(-event.deltaY * WHEEL_ZOOM_RATE);
      scene.changeView((view) => {
        if (event.ctrlKey || event.metaKey) return zoomTime(view, zoom, x);
        if (event.altKey) return zoomPitch(view, zoom, y);
        // Shift turns a mouse's vertical wheel into horizontal scrolling.
        const [dx, dy] = event.shiftKey
          ? [event.deltaY, event.deltaX]
          : [event.deltaX, event.deltaY];
        return {
          ...view,
          scrollTicks: view.scrollTicks + dx / view.pixelsPerTick,
          scrollY: view.scrollY + dy,
        };
      });
    };
    container.addEventListener("wheel", onWheel, { passive: false });
    return () => container.removeEventListener("wheel", onWheel);
  }, [scene]);

  const zoomTimeBy = (factor: number) =>
    scene.changeView((view) => {
      const area = noteArea(view);
      return zoomTime(view, factor, area.x + area.width / 2);
    });
  const zoomPitchBy = (factor: number) =>
    scene.changeView((view) => {
      const area = noteArea(view);
      return zoomPitch(view, factor, area.y + area.height / 2);
    });

  return (
    <section className="piano-roll" aria-label="Piano roll">
      <div className="piano-roll-canvas" ref={containerRef}>
        <canvas ref={gridRef} aria-hidden="true" />
        <canvas ref={notesRef} aria-hidden="true" />
        <canvas ref={topRef} aria-hidden="true" />
      </div>
      <div className="zoom" role="group" aria-label="Zoom">
        <button type="button" aria-label="Zoom out time" onClick={() => zoomTimeBy(1 / ZOOM_STEP)}>
          −
        </button>
        <span>Time</span>
        <button type="button" aria-label="Zoom in time" onClick={() => zoomTimeBy(ZOOM_STEP)}>
          +
        </button>
        <button
          type="button"
          aria-label="Zoom out pitch"
          onClick={() => zoomPitchBy(1 / ZOOM_STEP)}
        >
          −
        </button>
        <span>Pitch</span>
        <button type="button" aria-label="Zoom in pitch" onClick={() => zoomPitchBy(ZOOM_STEP)}>
          +
        </button>
      </div>
    </section>
  );
}

/** "light" or "dark", following the system, so the canvases redraw in the new colours. */
function useColourScheme(): "light" | "dark" {
  const [query] = useState(() =>
    typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-color-scheme: dark)")
      : null,
  );
  const [dark, setDark] = useState(() => query?.matches ?? false);
  useEffect(() => {
    if (!query) return;
    const onChange = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [query]);
  return dark ? "dark" : "light";
}
