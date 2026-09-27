import type { OutputView } from "./backend";

interface Props {
  output: OutputView;
  dropouts: number;
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

/** The output device, its buffer size and the dropout count. */
export function Output({ output, dropouts, busy, onBufferSize }: Props) {
  // The picked size stays listed even if this device doesn't support it.
  const sizes = [...new Set([...output.bufferSizes, output.requestedBufferSize])].sort(
    (a, b) => a - b,
  );
  return (
    <dl className="output">
      <dt>Output</dt>
      <dd data-testid="device">{describe(output)}</dd>

      <dt>
        <label htmlFor="buffer-size">Buffer</label>
      </dt>
      <dd>
        <select
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
        </select>
        {output.state === "running" && output.bufferSize !== output.requestedBufferSize && (
          <span className="note"> The device is using {output.bufferSize}.</span>
        )}
      </dd>

      <dt>Dropouts</dt>
      <dd data-testid="dropouts">{dropouts}</dd>
    </dl>
  );
}
