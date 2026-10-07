import { useEffect, useState } from "react";
import type { FrameStats, FrameSummary } from "./frameStats";
import "./FrameTime.css";

/** How often the readout updates. */
const REFRESH_MS = 500;

function ms(value: number): string {
  return value.toFixed(1);
}

/**
 * How long the piano roll's frames take, p50 and p99 over the last few
 * seconds: the time between frames (16.7 ms is 60 frames a second), and the
 * time spent drawing.
 */
export function FrameTime({ stats }: { stats: FrameStats }) {
  const [summary, setSummary] = useState<FrameSummary | null>(null);
  useEffect(() => {
    const timer = setInterval(() => setSummary(stats.summary()), REFRESH_MS);
    return () => clearInterval(timer);
  }, [stats]);
  return (
    <p className="frame-time" data-testid="frame-time">
      {summary
        ? `Frames ${ms(summary.intervalP50)} / ${ms(summary.intervalP99)} ms · ` +
          `drawing ${ms(summary.drawP50)} / ${ms(summary.drawP99)} ms (p50 / p99)`
        : "Frames: measuring…"}
    </p>
  );
}
