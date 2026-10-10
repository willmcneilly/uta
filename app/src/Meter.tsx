import { useEffect, useRef } from "react";
import { type Colours, readColours } from "./design/readColours";
import { useTokenVersion } from "./design/tokenChanges";
import { type MeterLevel, SILENT, meterFraction, nextShown } from "./meterLevel";
import "./Meter.css";

/** The master meter's height, and every meter's unless it says otherwise. */
const HEIGHT = 10;

interface Props {
  level: MeterLevel;
  /** The accessible name, such as "Level meter" or "Synth 2 meter". */
  label: string;
  /** In CSS pixels. */
  width?: number;
  /** In CSS pixels. */
  height?: number;
}

/**
 * A peak meter drawn on a canvas, once per screen frame. It holds each peak
 * for about a second, then falls. A new colour scheme, or a change from the
 * tuning panel, repaints it in the new colours.
 */
export function Meter({ level, label, width = 240, height = HEIGHT }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tokens = useTokenVersion();

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    const scale = window.devicePixelRatio || 1;
    canvas.width = width * scale;
    canvas.height = height * scale;
    context.scale(scale, scale);
    const colours = readColours(canvas);

    let shown = SILENT;
    let painted = NaN;
    let last = performance.now();
    let request = 0;
    const draw = (now: number) => {
      shown = nextShown(shown, level.take(), (now - last) / 1000);
      last = now;
      const fraction = meterFraction(shown.db);
      // Most meters sit still most of the time: only repaint when they move.
      if (fraction !== painted) {
        paint(context, width, height, fraction, colours);
        painted = fraction;
      }
      request = requestAnimationFrame(draw);
    };
    request = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(request);
  }, [level, width, height, tokens]);

  return (
    <canvas
      ref={canvasRef}
      className="meter"
      style={{ width, height }}
      role="img"
      aria-label={label}
    />
  );
}

function paint(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  fraction: number,
  colours: Colours,
): void {
  context.fillStyle = colours.line2;
  context.fillRect(0, 0, width, height);
  // Signal up to -18 dB, hot above. The scale stops at 0 dB, so clipping
  // shows on the clip light, not here.
  const zones: [number, string][] = [
    [meterFraction(-18), colours.signal],
    [1, colours.signalHot],
  ];
  let from = 0;
  for (const [to, colour] of zones) {
    const end = Math.min(to, fraction);
    if (end > from) {
      context.fillStyle = colour;
      context.fillRect(from * width, 0, (end - from) * width, height);
    }
    from = to;
  }
}
