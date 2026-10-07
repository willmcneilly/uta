import { useEffect, useRef } from "react";
import { type Colours, readColours } from "./design/readColours";
import { type MeterLevel, SILENT, meterFraction, nextShown } from "./meterLevel";
import { useColourScheme } from "./useColourScheme";
import "./Meter.css";

const HEIGHT = 10;

interface Props {
  level: MeterLevel;
  /** The accessible name, such as "Level meter" or "Synth 2 meter". */
  label: string;
  /** In CSS pixels. */
  width?: number;
}

/**
 * A peak meter drawn on a canvas, once per screen frame. It holds each peak
 * for about a second, then falls. A new colour scheme repaints it in the new
 * colours.
 */
export function Meter({ level, label, width = 240 }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const scheme = useColourScheme();

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    const scale = window.devicePixelRatio || 1;
    canvas.width = width * scale;
    canvas.height = HEIGHT * scale;
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
        paint(context, width, fraction, colours);
        painted = fraction;
      }
      request = requestAnimationFrame(draw);
    };
    request = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(request);
  }, [level, width, scheme]);

  return (
    <canvas
      ref={canvasRef}
      className="meter"
      style={{ width, height: HEIGHT }}
      role="img"
      aria-label={label}
    />
  );
}

function paint(
  context: CanvasRenderingContext2D,
  width: number,
  fraction: number,
  colours: Colours,
): void {
  context.fillStyle = colours.line2;
  context.fillRect(0, 0, width, HEIGHT);
  // Signal up to -18 dB, hot to -6 dB, clip above.
  const zones: [number, string][] = [
    [meterFraction(-18), colours.signal],
    [meterFraction(-6), colours.signalHot],
    [1, colours.clip],
  ];
  let from = 0;
  for (const [to, colour] of zones) {
    const end = Math.min(to, fraction);
    if (end > from) {
      context.fillStyle = colour;
      context.fillRect(from * width, 0, (end - from) * width, HEIGHT);
    }
    from = to;
  }
}
