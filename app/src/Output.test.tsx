import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OutputView } from "./backend";
import { MeterLevel } from "./meterLevel";
import { Output } from "./Output";
import { SLOWEST_BLOCK_REFRESH_MS } from "./slowestBlock";

const output: OutputView = {
  state: "running",
  device: "MacBook Pro Speakers",
  sampleRate: 48000,
  bufferSize: 64,
  requestedBufferSize: 64,
  bufferSizes: [32, 64, 128],
};

describe("Output", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const renderOutput = (slowestBlock: MeterLevel) =>
    render(
      <Output
        output={output}
        dropouts={0}
        slowestBlock={slowestBlock}
        busy={false}
        onBufferSize={() => {}}
      />,
    );

  it("shows the slowest block since its last refresh, next to the dropouts", () => {
    const slowestBlock = new MeterLevel();
    renderOutput(slowestBlock);
    const readout = screen.getByTestId("slowest-block");
    expect(readout).toHaveTextContent("–");

    // One slow block among quick ones is still what it shows.
    slowestBlock.push(0.04);
    slowestBlock.push(0.93);
    slowestBlock.push(0.05);
    act(() => vi.advanceTimersByTime(SLOWEST_BLOCK_REFRESH_MS));
    expect(readout).toHaveTextContent("93%");

    // The next refresh starts again.
    slowestBlock.push(0.031);
    act(() => vi.advanceTimersByTime(SLOWEST_BLOCK_REFRESH_MS));
    expect(readout).toHaveTextContent("3.1%");
  });
});
