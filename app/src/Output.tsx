import { useEffect, useState } from "react";
import type { OutputView } from "./backend";
import { Menu } from "./design/Menu";
import type { MeterLevel } from "./meterLevel";
import { percent, SLOWEST_BLOCK_REFRESH_MS } from "./slowestBlock";
import "./Output.css";

interface Props {
  output: OutputView;
  dropouts: number;
  /** The slowest block from each frame, as a share of its deadline. */
  slowestBlock: MeterLevel;
  /** Whether a buffer change is still being applied. */
  busy: boolean;
  onBufferSize: (size: number) => void;
}

function describe(output: OutputView): string {
  switch (output.state) {
    case "running": {
      const rate = output.sampleRate ? ` · ${output.sampleRate / 1000} kHz` : "";
      return `${output.device ?? "Unknown device"}${rate}`;
    }
    case "waiting":
      return "No output device. Waiting for one…";
    case "failed":
      return "Playback failed. Restart Uta.";
  }
}

/**
 * The audio thread's slowest block, as a share of its deadline. It shows the
 * slowest since its last refresh, so one slow block between refreshes is
 * still seen.
 */
function SlowestBlock({ load }: { load: MeterLevel }) {
  const [slowest, setSlowest] = useState<number | null>(null);
  useEffect(() => {
    const timer = setInterval(() => setSlowest(load.take()), SLOWEST_BLOCK_REFRESH_MS);
    return () => clearInterval(timer);
  }, [load]);
  return (
    <dd
      className="number"
      data-testid="slowest-block"
      title="The audio thread's slowest block, as a share of the time it has. Over 100% is late."
    >
      {slowest === null ? "–" : percent(slowest)}
    </dd>
  );
}

/** The output device, its buffer size, the dropout count and the slowest block. */
export function Output({ output, dropouts, slowestBlock, busy, onBufferSize }: Props) {
  // The picked size stays listed even if this device doesn't support it.
  const sizes = [...new Set([...output.bufferSizes, output.requestedBufferSize])].sort(
    (a, b) => a - b,
  );
  return (
    <dl className="output">
      <dt>Output</dt>
      <dd data-testid="device" data-state={output.state}>
        {describe(output)}
      </dd>

      <dt>
        <label htmlFor="buffer-size">Buffer</label>
      </dt>
      <dd>
        <Menu
          className="buffer"
          id="buffer-size"
          value={output.requestedBufferSize}
          disabled={busy}
          onChange={(event) => onBufferSize(Number(event.currentTarget.value))}
        >
          {sizes.map((size) => (
            <option key={size} value={size} disabled={!output.bufferSizes.includes(size)}>
              {size} samples
            </option>
          ))}
        </Menu>
        {output.state === "running" && output.bufferSize !== output.requestedBufferSize && (
          <span className="note"> The device is using {output.bufferSize}.</span>
        )}
      </dd>

      <dt>Dropouts</dt>
      <dd className="number" data-testid="dropouts">{dropouts}</dd>

      <dt>Slowest block</dt>
      <SlowestBlock load={slowestBlock} />
    </dl>
  );
}
