import { useEffect, useState } from "react";
import { type EchoSettings, barsFor, echoesOf, litAt, swayOf, sweepAt } from "./echoes";

// The Delay's drawing of its echoes: a tick per echo, as tall as it's loud,
// left above the line and right below, so stereo is mirrored and ping-pong
// bounces from lane to lane. It spans as many whole bars as the echoes last,
// with bar and beat lines and no numbers. While the song plays, or while
// you're pointing at the Delay or changing it, an orange sweep crosses it and
// each echo lights as it sounds. Read only: the knobs are the controls.
// SVG, coloured by the tokens through CSS (DelayPanel.css). provisional: D-31

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
}

/** Where the light is: the song's time within the drawing, in seconds, once a frame. */
function useLight(
  songSeconds: ((now: number) => number) | null,
  on: boolean,
  span: number,
): number | null {
  const [t, setT] = useState<number | null>(null);
  useEffect(() => {
    if (!on) {
      setT(null);
      return;
    }
    let start: number | null = null;
    let request = requestAnimationFrame(function frame(now) {
      start ??= now;
      const seconds = songSeconds ? songSeconds(now) : (now - start) / 1000;
      setT(((seconds % span) + span) % span);
      request = requestAnimationFrame(frame);
    });
    return () => cancelAnimationFrame(request);
  }, [songSeconds, on, span]);
  return t;
}

export function EchoDrawing({
  settings,
  wobble,
  enabled,
  barSeconds,
  beatsPerBar,
  songSeconds,
  preview,
}: Props) {
  const bars = barsFor(settings, barSeconds);
  const span = bars * barSeconds;
  const echoes = echoesOf(settings, span);
  const lighting = !enabled ? null : songSeconds ? "playing" : preview ? "preview" : null;
  const t = useLight(songSeconds, lighting !== null, span);

  const width = ECHO_WIDTH;
  const height = ECHO_HEIGHT;
  const x = (seconds: number) => 0.5 + (seconds / span) * (width - 1);
  const mid = height / 2;
  const reach = mid - 2;
  const beats = Array.from({ length: bars * beatsPerBar + 1 }, (_, beat) => beat);
  const beatSeconds = barSeconds / beatsPerBar;

  /** A tick at `tx`, `h` tall on the side or sides given. */
  const tick = (tx: number, h: number, side: "both" | "left" | "right") => (
    <>
      {side !== "right" && <line x1={tx} x2={tx} y1={mid - h} y2={mid} />}
      {side !== "left" && <line x1={tx} x2={tx} y1={mid} y2={mid + h} />}
    </>
  );

  const sweep = t === null ? null : sweepAt(t, echoes, settings.spacing);
  return (
    <svg
      className="echo-drawing"
      data-off={!enabled || undefined}
      data-lighting={(t !== null && lighting) || undefined}
      data-bars={bars}
      viewBox={`0 0 ${width} ${height}`}
      width={width}
      height={height}
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
          y2={height}
        />
      ))}
      <line className="base" x1={0} x2={width} y1={mid} y2={mid} />
      <text className="axis" x={3} y={9}>
        L
      </text>
      <text className="axis" x={3} y={height - 2}>
        R
      </text>
      {/* The note going in, quiet. */}
      <g className="dry">{tick(x(0) + 1, reach, "both")}</g>
      {t !== null && litAt(t, 0) > 0 && (
        <g className="dry sounding" style={{ opacity: litAt(t, 0) }}>
          {tick(x(0) + 1, reach, "both")}
        </g>
      )}
      {echoes.map((echo, n) => {
        const tx = x(echo.seconds) + swayOf(n, wobble);
        const h = echo.level * reach;
        const glow = t === null ? 0 : litAt(t, echo.seconds);
        return (
          <g key={n}>
            <g className="echo" data-side={echo.side} data-level={echo.level.toFixed(3)} data-x={tx.toFixed(1)}>
              {tick(tx, h, echo.side)}
            </g>
            {glow > 0 && (
              <g className="echo sounding" style={{ opacity: glow }}>
                {tick(tx, h, echo.side)}
              </g>
            )}
          </g>
        );
      })}
      {t !== null && sweep && (
        <g className="sweep">{tick(x(t), sweep.level * reach, sweep.side)}</g>
      )}
    </svg>
  );
}
