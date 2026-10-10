import { useEffect, useRef, useState } from "react";
import type { TestSong } from "../backend";
import { type Report, formatReport } from "./report";
import type { Progress } from "./run";
import "./Benchmark.css";

const SONG_NAMES: Record<TestSong, string> = {
  heavy: "heavy",
  wide: "wide",
  "check-7": "check 7",
};

type Phase =
  | { name: "asking" }
  | { name: "running"; progress: Progress | null }
  | { name: "done"; text: string; copy: "ready" | "copied" | "failed" }
  | { name: "failed"; reason: string };

interface Props {
  /** Runs the benchmark, telling `onProgress` what it's doing. */
  run: (onProgress: (progress: Progress) => void) => Promise<Report>;
  onClose: () => void;
}

/**
 * Develop → Run Benchmark: asks first, because it replaces the current song,
 * then shows what it's doing and, at the end, the report, to copy into a PR.
 */
export function Benchmark({ run, onClose }: Props) {
  const [phase, setPhase] = useState<Phase>({ name: "asking" });
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const start = () => {
    setPhase({ name: "running", progress: null });
    run((progress) => mounted.current && setPhase({ name: "running", progress })).then(
      (report) =>
        mounted.current && setPhase({ name: "done", text: formatReport(report), copy: "ready" }),
      (reason: unknown) => mounted.current && setPhase({ name: "failed", reason: String(reason) }),
    );
  };

  const copy = (text: string) => {
    navigator.clipboard
      .writeText(text)
      .then(() => mounted.current && setPhase({ name: "done", text, copy: "copied" }))
      // The report stays, to select and copy by hand.
      .catch(() => mounted.current && setPhase({ name: "done", text, copy: "failed" }));
  };

  return (
    <div className="benchmark-backdrop">
      <section
        className="benchmark"
        role="dialog"
        aria-modal="true"
        aria-labelledby="benchmark-title"
      >
        <h2 id="benchmark-title">Benchmark</h2>
        {phase.name === "asking" && (
          <>
            <p>
              This replaces the current song with three test songs in turn (heavy, wide and check
              7), and times slider steps, note deletes and a drag on each. It takes a minute or
              two, and the window is busy while it runs.
            </p>
            <div className="benchmark-actions">
              <button type="button" onClick={onClose}>
                Cancel
              </button>
              <button type="button" className="go" onClick={start} autoFocus>
                Replace and Run
              </button>
            </div>
          </>
        )}
        {phase.name === "running" && (
          <p role="status">
            {phase.progress
              ? `Running: ${SONG_NAMES[phase.progress.song]}, ${phase.progress.doing}…`
              : "Starting…"}
          </p>
        )}
        {phase.name === "done" && (
          <>
            <pre className="benchmark-report" aria-label="Benchmark report">
              {phase.text}
            </pre>
            <div className="benchmark-actions">
              <button type="button" onClick={() => copy(phase.text)}>
                {phase.copy === "copied"
                  ? "Copied"
                  : phase.copy === "failed"
                    ? "Couldn't Copy: Select the Text"
                    : "Copy as Text"}
              </button>
              <button type="button" onClick={onClose}>
                Close
              </button>
            </div>
          </>
        )}
        {phase.name === "failed" && (
          <>
            <p className="error" role="alert">
              The benchmark stopped: {phase.reason}
            </p>
            <div className="benchmark-actions">
              <button type="button" onClick={onClose}>
                Close
              </button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
