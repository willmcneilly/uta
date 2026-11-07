import { type PointerEvent, useEffect, useLayoutEffect, useRef } from "react";
import type { FrameLoop } from "../frameLoop";
import { useHatch } from "../synth/hatch";
import { nextGesture } from "../useGesture";
import {
  HIGH_HZ,
  LOW_HZ,
  type Reflection,
  type TailSettings,
  decayFor,
  endSeconds,
  formatTime,
  lateAt,
  random,
  reflectionsOf,
  secondsAt,
  silentAt,
  tailAt,
  tailEndSeconds,
  xOf,
} from "./tail";

// The Reverb's drawing: one hatched shape rising from a baseline, so up only
// ever means louder, on a time scale every sound shares. Its edge is worked
// out from the three bands: it lasts as long as the longest, it's rough while
// the highs still ring and smooths as they fade, and it swells slowly where
// the lows hold on. The early reflections are ticks in front of it, as in
// the Delay's drawing, so Depth reads. Touching a low or high setting shows
// that band's own edge, dashed, for a moment, with its length. Dragging the
// end sets Decay. While it sounds (the song playing, or previewed while
// you're pointing at the panel, as the Delay does), the ticks light as they
// arrive and an orange line runs under the edge. SVG, coloured by the tokens
// through CSS (ReverbPanel.css). React draws the shape when the settings
// change; the light moves once a screen frame on the app's frame loop, so a
// playing Reverb doesn't re-render. From the design sandbox's round 08e,
// 06b (`live/src/envelope/refined.tsx`). provisional: D-40

/** The drawing's size in CSS pixels, the Delay's drawing's. provisional: D-40 */
export const TAIL_WIDTH = 224;
export const TAIL_HEIGHT = 52;

/** The low or high band, whose own edge shows while one of its settings is touched. */
export type Band = "low" | "high";

interface Props {
  settings: TailSettings;
  /** Off, it's drawn in the third ink and never lights. */
  enabled: boolean;
  /** One bar at the song's tempo, in seconds: while it plays, a note goes in each bar. */
  barSeconds: number;
  /** Where the song is now, in seconds, while it plays; null when it's stopped. */
  songSeconds: ((now: number) => number) | null;
  /** You're pointing at the panel, or a control in it has focus: stopped, it plays through once. */
  preview: boolean;
  /** The band whose setting was just touched, or null. */
  touched: Band | null;
  /** Decay's limits, Rust's, for dragging the end. */
  decayLimits: [number, number];
  /** Sets Decay from dragging the end. `gesture` is the same for every change in one drag. */
  onDecay: (seconds: number, gesture: number) => void;
  /** Decay as the end is dragged, before Rust replies, then null when the drag ends. */
  onDrag: (seconds: number | null) => void;
  /** The app's drawing loop, which moves the light once a screen frame. */
  frames: FrameLoop;
}

const WIDTH = TAIL_WIDTH;
const HEIGHT = TAIL_HEIGHT;
const TOP = 4;
const FLOOR = HEIGHT - 2;
const REACH = FLOOR - TOP;
/** How long a tick stays lit after its reflection arrives, in seconds. */
const LIT_SECONDS = 0.06;
/** The longest a preview runs before it starts again, so a 30 s tail doesn't hold it. */
const LONGEST_PREVIEW = 8;
/** The gap after the tail, before a preview starts again, in seconds. */
const PREVIEW_GAP = 0.6;

/** The height of a level from 0 to 1. */
const y = (level: number) => FLOOR - level * REACH;

type Point = [number, number];

const pathOf = (points: Point[]) =>
  points.map(([px, py], i) => `${i === 0 ? "M" : "L"}${px.toFixed(2)},${py.toFixed(2)}`).join(" ");

/** The tail's edge from its start to its end: roughened by the highs, swelling with the lows. */
function edgeOf(s: TailSettings, start: number, end: number): Point[] {
  const next = random(6);
  const edge: Point[] = [[start, FLOOR]];
  // A gentle grain: each step's roughness is the mean of two draws, every 2 px.
  for (let x = Math.ceil(start + 0.01), i = 0; x <= Math.min(WIDTH, end + 1); x += 2, i += 2) {
    const { low, high, all } = tailAt(s, secondsAt(x, WIDTH));
    const rough = all > 0 ? (high / all) * 0.16 * ((next() + next()) / 2) : 0;
    const swell = all > 0 ? (low / all) * 0.1 * (0.5 - 0.5 * Math.cos(i * 0.16)) : 0;
    edge.push([x, y(all * (1 - rough - swell))]);
  }
  edge.push([edge[edge.length - 1][0], FLOOR]);
  return edge;
}

/** One band's own edge, smooth. */
function bandEdgeOf(s: TailSettings, hz: number, start: number, end: number): Point[] {
  const points: Point[] = [];
  for (let x = Math.ceil(start + 0.01); x <= Math.min(WIDTH, end + 1); x += 1) {
    points.push([x, y(lateAt(s, hz, secondsAt(x, WIDTH)))]);
  }
  return points;
}

/** The reflections to draw: those closer than 2 px to the one before would merge into a block. */
function ticksOf(reflections: Reflection[]): Reflection[] {
  return reflections.filter(
    (r, i) => i === 0 || xOf(r.seconds, WIDTH) - xOf(reflections[i - 1].seconds, WIDTH) >= 2,
  );
}

/** What the light needs from the latest render. */
interface Drawn {
  settings: TailSettings;
  end: number;
}

export function TailDrawing({
  settings,
  enabled,
  barSeconds,
  songSeconds,
  preview,
  touched,
  decayLimits,
  onDecay,
  onDrag,
  frames,
}: Props) {
  const [hatch, pattern] = useHatch();
  const end = endSeconds(settings);
  const endX = xOf(end, WIDTH);
  // The handle drags the tail's end, which Decay sets, so it sits there: on
  // the shape's end, unless the early burst outlasts the tail. With no tail
  // there's nothing for it to drag.
  const tailEnd = tailEndSeconds(settings);
  const handleX = tailEnd === null ? null : xOf(tailEnd, WIDTH);
  const start = xOf(settings.predelaySeconds, WIDTH);
  const edge = edgeOf(settings, start, endX);
  const ticks = ticksOf(reflectionsOf(settings));
  const lighting = !enabled ? null : songSeconds ? "playing" : preview ? "preview" : null;

  const svg = useRef<SVGSVGElement>(null);
  const drawn = useRef<Drawn>({ settings, end });
  // The light reads the latest shape without restarting.
  useLayoutEffect(() => {
    drawn.current = { settings, end };
  });

  useEffect(() => {
    const root = svg.current;
    if (!root || lighting === null) return;
    let first: number | null = null;
    const sweep = root.querySelector<SVGLineElement>(".sweep");
    const remove = frames.add((now) => {
      const { settings, end } = drawn.current;
      first ??= now;
      const t = songSeconds
        ? ((songSeconds(now) % barSeconds) + barSeconds) % barSeconds
        : ((now - first) / 1000) % (Math.min(end, LONGEST_PREVIEW) + PREVIEW_GAP);
      root.dataset.lighting = lighting;
      for (const tick of root.querySelectorAll<SVGLineElement>(".early line")) {
        const at = Number(tick.dataset.at);
        const lit = t >= at && t < at + LIT_SECONDS;
        if (lit !== (tick.dataset.lit !== undefined)) {
          if (lit) tick.dataset.lit = "";
          else delete tick.dataset.lit;
        }
      }
      if (!sweep) return;
      if (t > settings.predelaySeconds && t <= end) {
        const x = String(xOf(t, WIDTH));
        sweep.setAttribute("x1", x);
        sweep.setAttribute("x2", x);
        sweep.setAttribute("y2", String(y(tailAt(settings, t).all)));
        sweep.style.visibility = "";
      } else {
        sweep.style.visibility = "hidden";
      }
    });
    return () => {
      remove();
      delete root.dataset.lighting;
      for (const tick of root.querySelectorAll<SVGLineElement>(".early line")) delete tick.dataset.lit;
      if (sweep) sweep.style.visibility = "hidden";
    };
  }, [frames, lighting, songSeconds, barSeconds]);

  // Dragging the end sets Decay, so the end follows the pointer.
  const latest = useRef({ settings, decayLimits, onDecay, onDrag });
  useLayoutEffect(() => {
    latest.current = { settings, decayLimits, onDecay, onDrag };
  });
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);
  const startDrag = (event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    stopDrag.current?.();
    const gesture = nextGesture();
    const move = (move: { clientX: number }) => {
      const box = svg.current?.getBoundingClientRect();
      if (!box || box.width === 0) return;
      const x = ((move.clientX - box.left) / box.width) * WIDTH;
      const { settings, decayLimits, onDecay, onDrag } = latest.current;
      const decay = decayFor(settings, secondsAt(x, WIDTH), decayLimits);
      onDrag(decay);
      onDecay(decay, gesture);
    };
    const stop = () => {
      latest.current.onDrag(null);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      stopDrag.current = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    stopDrag.current = stop;
  };

  const bandHz = touched === "low" ? LOW_HZ : HIGH_HZ;
  // The end's length beside it, or tucked under the top right when there's no room.
  const tucked = endX > WIDTH - 40;

  return (
    <svg
      ref={svg}
      className="tail-drawing"
      data-off={!enabled || undefined}
      data-end={end.toFixed(3)}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      width={WIDTH}
      height={HEIGHT}
      role="img"
      aria-label="The reverb's shape"
    >
      {pattern}
      <line className="base" x1={0} x2={WIDTH} y1={FLOOR + 0.5} y2={FLOOR + 0.5} />
      <path className="under" d={`${pathOf(edge)} Z`} fill={hatch} />
      <path className="edge" d={pathOf(edge.slice(1, -1))} />
      {touched && (
        <>
          <path
            className="band-edge"
            data-band={touched}
            d={pathOf(bandEdgeOf(settings, bandHz, start, endX))}
          />
          <text className="label" x={2} y={10}>
            {touched === "low" ? "Lows" : "Highs"} {formatTime(silentAt(settings, bandHz))}
          </text>
        </>
      )}
      <g className="early">
        {ticks.map((tick, n) => {
          const x = xOf(tick.seconds, WIDTH);
          return <line key={n} data-at={tick.seconds} x1={x} x2={x} y1={FLOOR} y2={y(tick.level)} />;
        })}
      </g>
      <line className="sweep" y1={FLOOR} style={{ visibility: "hidden" }} />
      <text
        className="label end-time"
        x={tucked ? WIDTH - 2 : endX + 8}
        y={tucked ? TOP + 8 : FLOOR - 4}
        textAnchor={tucked ? "end" : "start"}
      >
        {formatTime(end)}
      </text>
      {handleX !== null && (
        <g className="end-handle" onPointerDown={startDrag}>
          <title>Drag to set how long it rings</title>
          <circle className="hit" cx={handleX} cy={FLOOR} r={9} />
          <circle className="dot" cx={handleX} cy={FLOOR} r={3.5} />
        </g>
      )}
    </svg>
  );
}
