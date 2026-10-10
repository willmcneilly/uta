import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Fader } from "./Fader";
import { press, RAIL_WIDTH, thumbX } from "./faderTesting";
import { describeNumberControl } from "./numberControlBehaviour";
import { linearScale } from "./numberScale";
import type { NumberControlOptions } from "./useNumberControl";

describeNumberControl("Fader", {
  render: (options) => {
    const view = render(<Fader label="Volume" {...options} />);
    return {
      control: screen.getByRole("slider", { name: "Volume" }),
      rerender: (next: NumberControlOptions) => view.rerender(<Fader label="Volume" {...next} />),
    };
  },
  press: (fader, { offset = 0, ...keys } = {}) => {
    const drag = press(fader, { fraction: (thumbX(fader) + offset) / RAIL_WIDTH, ...keys });
    const along = {
      by: (pixels: number, keys = {}) => {
        drag.by(pixels, keys);
        return along;
      },
      release: drag.release,
    };
    return along;
  },
  travel: RAIL_WIDTH,
});

describeNumberControl("Fader stood on end", {
  render: (options) => {
    const view = render(<Fader label="Level" orientation="vertical" {...options} />);
    return {
      control: screen.getByRole("slider", { name: "Level" }),
      rerender: (next: NumberControlOptions) =>
        view.rerender(<Fader label="Level" orientation="vertical" {...next} />),
    };
  },
  press: (fader, { offset = 0, ...keys } = {}) => {
    const drag = press(fader, { fraction: (thumbX(fader) + offset) / RAIL_WIDTH, ...keys });
    const along = {
      by: (pixels: number, keys = {}) => {
        drag.by(pixels, keys);
        return along;
      },
      release: drag.release,
    };
    return along;
  },
  travel: RAIL_WIDTH,
});

// A volume from -60 to 0 dB in 0.5 dB steps: 120 positions over the test
// rail's 1000 pixels.
const scale = linearScale(-60, 0, 0.5);
const formatDb = (db: number) => `${db.toFixed(1)} dB`;

function renderFader(value = -12, defaultValue = -12) {
  const onChange = vi.fn<(value: number, gesture?: number) => void>();
  render(
    <Fader
      label="Volume"
      value={value}
      defaultValue={defaultValue}
      scale={scale}
      format={formatDb}
      onChange={onChange}
    />,
  );
  return { fader: screen.getByRole("slider", { name: "Volume" }), onChange };
}

describe("linearScale", () => {
  it("maps whole positions to values, exactly at the ends and on the step", () => {
    const pan = linearScale(-1, 1, 0.01);
    expect(pan.steps).toBe(200);
    expect(pan.fromPosition(0)).toBe(-1);
    expect(pan.fromPosition(50)).toBe(-0.5);
    expect(pan.fromPosition(200)).toBe(1);
    expect(pan.toPosition(0.25)).toBe(125);
    expect(pan.toPosition(5)).toBe(200);
    expect(pan.toPosition(-5)).toBe(0);
  });
});

describe("Fader", () => {
  it("is a horizontal slider, named by its label", () => {
    const { fader } = renderFader();
    expect(fader).toHaveAttribute("aria-orientation", "horizontal");
  });

  it("takes an accessible name of its own when given one", () => {
    render(
      <Fader
        label="Vol"
        ariaLabel="Synth 1 volume"
        value={0}
        defaultValue={0}
        scale={scale}
        format={formatDb}
        onChange={() => {}}
      />,
    );
    expect(screen.getByRole("slider", { name: "Synth 1 volume" })).toBeInTheDocument();
  });

  it("shows its value in an output beside it", () => {
    renderFader(-20);
    expect(screen.getByRole("status")).toHaveTextContent("-20.0 dB");
  });

  it("draws its thumb at its value, and moves it with the mouse along the rail", () => {
    const { fader } = renderFader(-30);
    expect(thumbX(fader)).toBe(500);
    const drag = press(fader).to(0.75);
    expect(thumbX(fader)).toBe(750);
    drag.release();
  });

  it("stays under the mouse after the mouse leaves an end", () => {
    const { fader, onChange } = renderFader(-30);
    const drag = press(fader).to(1.2);
    expect(fader).toHaveAttribute("aria-valuenow", "0");
    drag.to(1.1);
    expect(onChange.mock.calls.map(([value]) => value)).toEqual([0]);
    drag.to(0.5).release();
    expect(onChange.mock.calls.map(([value]) => value)).toEqual([0, -30]);
  });

  it("marks its default on the rail", () => {
    const { fader } = renderFader(-30, -12);
    const tick = fader.querySelector<HTMLElement>(".fader-default");
    expect(tick?.style.left).toBe("80%");
  });

  describe("stood on end", () => {
    function renderUpright(value = -30, defaultValue = -12) {
      const onChange = vi.fn<(value: number, gesture?: number) => void>();
      render(
        <Fader
          label="Level"
          orientation="vertical"
          value={value}
          defaultValue={defaultValue}
          scale={scale}
          format={formatDb}
          onChange={onChange}
        />,
      );
      return { fader: screen.getByRole("slider", { name: "Level" }), onChange };
    }

    it("is a vertical slider", () => {
      const { fader } = renderUpright();
      expect(fader).toHaveAttribute("aria-orientation", "vertical");
    });

    it("draws its thumb and default up from the bottom, and goes up as the mouse does", () => {
      const { fader, onChange } = renderUpright(-30, -12);
      expect(fader.querySelector<HTMLElement>(".fader-thumb")?.style.bottom).toBe("50%");
      expect(fader.querySelector<HTMLElement>(".fader-default")?.style.bottom).toBe("80%");
      // Sideways does nothing; upwards raises it.
      const drag = press(fader);
      fireEvent.pointerMove(window, { clientX: 400, clientY: -500 });
      expect(onChange).not.toHaveBeenCalled();
      drag.to(0.75);
      expect(thumbX(fader)).toBe(750);
      drag.release();
      expect(onChange.mock.calls.map(([value]) => value)).toEqual([-15]);
    });
  });
});
