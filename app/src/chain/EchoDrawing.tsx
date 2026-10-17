import { useEffect, useLayoutEffect, useRef } from "react";
import type { FrameLoop } from "../frameLoop";
import { type Echo, type EchoSettings, barsFor, echoesOf, litAt, swayOf, sweepAt } from "./echoes";

// The Delay's drawing of its echoes: a tick per echo, as tall as it's loud,
// left above the line and right below, so stereo is mirrored and ping-pong
// bounces from lane to lane. It spans as many whole bars as the echoes last,
// with bar and beat lines and no numbers. While the song plays, or while
// you're pointing at the Delay or changing it, an orange sweep crosses it and
// each echo lights as it sounds. Read only: the knobs are the controls.
// SVG, coloured by the tokens through CSS (DelayPanel.css). React draws the
// echoes when the settings change; the light moves once a screen frame on
// the app's frame loop, setting only the lit marks' opacity and the sweep's
// line, so a playing Delay doesn't re-render. provisional: D-31

/** The drawing's size in CSS pixels. provisional: D-31 */
export const ECHO_WIDTH = 224;
export const ECHO_HEIGHT = 52;

interface Props {
  settings: EchoSettings;
  /** From 0 to 1: sways the echoes slightly off the grid. */
  wobble: number;
  /** Off, it's drawn in the third ink and never lights. */
  enabled: boolean;
  /** One bar at the song's tempo, in seconds. */
  barSeconds: number;
  beatsPerBar: number;
  /**
   * Where the song is now, in seconds, while it plays, or null when it's
   * stopped. Read once a screen frame, with the frame's time.
   */
  songSeconds: ((now: number) => number) | null;
  /**
   * You're pointing at the Delay, or a control in it has focus: when the song
   * is stopped, the echoes light as if a note had just gone in.
   */
  preview: boolean;
  /** The app's drawing loop, which moves the light once a screen frame. */
  frames: FrameLoop;
}

const WIDTH = ECHO_WIDTH;
const HEIGHT = ECHO_HEIGHT;
const MID = HEIGHT / 2;
const REACH = MID - 2;

/** What the light needs from the latest render. */
interface Drawn {
  echoes: Echo[];
  spacing: number;
  span: number;
}

/** Sets a tick's two lines: `h` tall at `x`, on the side or sides given. */
function placeTick(up: SVGLineElement, down: SVGLineElement, x: number, h: number, side: Echo["side"]) {
  for (const line of [up, down]) {
    line.setAttribute("x1", String(x));
    line.setAttribute("x2", String(x));
  }
  up.setAttribute("y1", String(MID - h));
  up.setAttribute("y2", String(MID));
  down.setAttribute("y1", String(MID));
  down.setAttribute("y2", String(MID + h));
  up.style.visibility = side === "right" ? "hidden" : "";
  down.style.visibility = side === "left" ? "hidden" : "";
}

export function EchoDrawing({
  settings,
  wobble,
  enabled,
  barSeconds,
  beatsPerBar,
  songSeconds,
  preview,
  frames,
}: Props) {
  const bars = barsFor(settings, barSeconds);
  const span = bars * barSeconds;
  const echoes = echoesOf(settings, span);
  const lighting = !enabled ? null : songSeconds ? "playing" : preview ? "preview" : null;

  const svg = useRef<SVGSVGElement>(null);
  const drawn = useRef<Drawn>({ echoes, spacing: settings.spacing, span });
  // The light reads the latest echoes without restarting.
  useLayoutEffect(() => {
    drawn.current = { echoes, spacing: settings.spacing, span };
  });

  useEffect(() => {
    const root = svg.current;
    if (!root || lighting === null) return;
    let start: number | null = null;
    const sweep = root.querySelector<SVGGElement>(".sweep");
    const [up, down] = Array.from(root.querySelectorAll<SVGLineElement>(".sweep line"));
    const remove = frames.add((now) => {
      const { echoes, spacing, span } = drawn.current;
      start ??= now;
      const seconds = songSeconds ? songSeconds(now) : (now - start) / 1000;
      const t = ((seconds % span) + span) % span;
      root.dataset.lighting = lighting;
      for (const mark of root.querySelectorAll<SVGGElement>(".sounding")) {
        mark.style.opacity = String(litAt(t, Number(mark.dataset.at)));
      }
      const { level, side } = sweepAt(t, echoes, spacing);
      if (sweep && up && down) {
        sweep.style.visibility = "";
        placeTick(up, down, 0.5 + (t / span) * (WIDTH - 1), level * REACH, side);
      }
    });
    return () => {
      remove();
      delete root.dataset.lighting;
      for (const mark of root.querySelectorAll<SVGGElement>(".sounding")) mark.style.opacity = "0";
      if (sweep) sweep.style.visibility = "hidden";
    };
  }, [frames, lighting, songSeconds]);

  const x = (seconds: number) => 0.5 + (seconds / span) * (WIDTH - 1);
  const beats = Array.from({ length: bars * beatsPerBar + 1 }, (_, beat) => beat);
  const beatSeconds = barSeconds / beatsPerBar;

  /** A tick at `tx`, `h` tall on the side or sides given. */
  const tick = (tx: number, h: number, side: Echo["side"]) => (
    <>
      {side !== "right" && <line x1={tx} x2={tx} y1={MID - h} y2={MID} />}
      {side !== "left" && <line x1={tx} x2={tx} y1={MID} y2={MID + h} />}
    </>
  );

  return (
    <svg
      ref={svg}
      className="echo-drawing"
      data-off={!enabled || undefined}
      data-bars={bars}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      width={WIDTH}
      height={HEIGHT}
      role="img"
      aria-label="The echoes"
    >
      {beats.map((beat) => (
        <line
          key={beat}
          className={beat % beatsPerBar === 0 ? "bar" : "beat"}
          x1={x(beat * beatSeconds)}
          x2={x(beat * beatSeconds)}
          y1={0}
          y2={HEIGHT}
        />
      ))}
      <line className="base" x1={0} x2={WIDTH} y1={MID} y2={MID} />
      <text className="axis" x={3} y={9}>
        L
      </text>
      <text className="axis" x={3} y={HEIGHT - 2}>
        R
      </text>
      {/* The note going in, quiet, and lit as it goes in. */}
      <g className="dry">{tick(x(0) + 1, REACH, "both")}</g>
      <g className="dry sounding" data-at={0} style={{ opacity: 0 }}>
        {tick(x(0) + 1, REACH, "both")}
      </g>
      {echoes.map((echo, n) => {
        const tx = x(echo.seconds) + swayOf(n, wobble);
        const h = echo.level * REACH;
        return (
          <g key={n}>
            <g className="echo" data-side={echo.side} data-level={echo.level.toFixed(3)} data-x={tx.toFixed(1)}>
              {tick(tx, h, echo.side)}
            </g>
            <g className="echo sounding" data-at={echo.seconds} style={{ opacity: 0 }}>
              {tick(tx, h, echo.side)}
            </g>
          </g>
        );
      })}
      <g className="sweep" style={{ visibility: "hidden" }}>
        <line />
        <line />
      </g>
    </svg>
  );
}
