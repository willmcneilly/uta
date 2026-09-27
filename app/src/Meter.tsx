import { useEffect, useRef } from "react";
import { METER_FLOOR_DB, type MeterLevel, meterFraction, nextShownDb } from "./meterLevel";

const WIDTH = 240;
const HEIGHT = 10;

/** A peak meter drawn on a canvas, once per screen frame. */
export function Meter({ level }: { level: MeterLevel }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    const scale = window.devicePixelRatio || 1;
    canvas.width = WIDTH * scale;
    canvas.height = HEIGHT * scale;
    context.scale(scale, scale);
    const track = getComputedStyle(canvas).getPropertyValue("--meter-track") || "#333";

    let shownDb = METER_FLOOR_DB;
    let last = performance.now();
    let request = 0;
    const draw = (now: number) => {
      shownDb = nextShownDb(shownDb, level.take(), (now - last) / 1000);
      last = now;
      paint(context, meterFraction(shownDb), track);
      request = requestAnimationFrame(draw);
    };
    request = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(request);
  }, [level]);

  return (
    <canvas
      ref={canvasRef}
      className="meter"
      style={{ width: WIDTH, height: HEIGHT }}
      role="img"
      aria-label="Level meter"
    />
  );
}

function paint(context: CanvasRenderingContext2D, fraction: number, track: string): void {
  context.fillStyle = track;
  context.fillRect(0, 0, WIDTH, HEIGHT);
  // Green up to -18 dB, amber to -6 dB, red above.
  const zones: [number, string][] = [
    [meterFraction(-18), "#3fb950"],
    [meterFraction(-6), "#d29922"],
    [1, "#f85149"],
  ];
  let from = 0;
  for (const [to, colour] of zones) {
    const end = Math.min(to, fraction);
    if (end > from) {
      context.fillStyle = colour;
      context.fillRect(from * WIDTH, 0, (end - from) * WIDTH, HEIGHT);
    }
    from = to;
  }
}
