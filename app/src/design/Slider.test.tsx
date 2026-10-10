import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Slider } from "./Slider";
import { press, slide, thumbX } from "./sliderTesting";
import { FINE_DRAG, linearScale } from "./sliderScale";

// A volume from -60 to 0 dB in 0.5 dB steps: 120 positions over the test
// rail's 1000 pixels.
const scale = linearScale(-60, 0, 0.5);
const formatDb = (db: number) => `${db.toFixed(1)} dB`;

function renderSlider(value = -12, defaultValue = -12) {
  const onChange = vi.fn<(value: number, gesture?: number) => void>();
  const props = { label: "Volume", defaultValue, scale, format: formatDb, onChange };
  const view = render(<Slider {...props} value={value} />);
  const slider = screen.getByRole("slider", { name: "Volume" });
  /** Rust sends a new project value. */
  const project = (next: number) => view.rerender(<Slider {...props} value={next} />);
  return { slider, onChange, project };
}

const sent = (onChange: ReturnType<typeof renderSlider>["onChange"]) =>
  onChange.mock.calls.map(([value]) => value);
const gestures = (onChange: ReturnType<typeof renderSlider>["onChange"]) =>
  onChange.mock.calls.map(([, gesture]) => gesture);
const output = () => screen.getByRole("status");

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

describe("Slider", () => {
  it("is a slider to assistive tech, with its value and range", () => {
    const { slider } = renderSlider(-20);
    expect(slider).toHaveAttribute("aria-valuemin", "-60");
    expect(slider).toHaveAttribute("aria-valuemax", "0");
    expect(slider).toHaveAttribute("aria-valuenow", "-20");
    expect(slider).toHaveAttribute("aria-valuetext", "-20.0 dB");
    expect(slider).toHaveAttribute("tabindex", "0");
    expect(output()).toHaveTextContent("-20.0 dB");
  });

  it("takes an accessible name of its own when given one", () => {
    render(
      <Slider
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

  it("shows its own value during a drag, and the project's after", () => {
    const { slider, onChange, project } = renderSlider(-30);
    const drag = press(slider).to(0.75);
    // Rust hasn't replied: the slider still shows where the mouse is.
    expect(sent(onChange)).toEqual([-15]);
    expect(slider).toHaveAttribute("aria-valuenow", "-15");
    expect(output()).toHaveTextContent("-15.0 dB");
    expect(thumbX(slider)).toBe(750);
    // A reply to an older step doesn't move it back.
    project(-25);
    expect(slider).toHaveAttribute("aria-valuenow", "-15");

    drag.release();
    expect(slider).toHaveAttribute("aria-valuenow", "-25");
    expect(output()).toHaveTextContent("-25.0 dB");
    project(-15);
    expect(slider).toHaveAttribute("aria-valuenow", "-15");
  });

  it("sends each new step of a drag once, all with the drag's gesture", () => {
    const { slider, onChange } = renderSlider(-30);
    press(slider).to(0.25).by(1).to(0.5).release();
    slide(slider, 0.75);
    // 1 pixel is less than half a step, so it sends nothing.
    expect(sent(onChange)).toEqual([-45, -30, -15]);
    const [first, second, third] = gestures(onChange);
    expect(first).toEqual(expect.any(Number));
    expect(second).toBe(first);
    expect(third).toEqual(expect.any(Number));
    expect(third).not.toBe(first);
  });

  it("picks the thumb up where it is, and jumps to a press anywhere else", () => {
    const { slider, onChange } = renderSlider(-30);
    // The thumb is halfway: a press 4 pixels off it sends nothing.
    press(slider, { fraction: 0.504 }).release();
    expect(onChange).not.toHaveBeenCalled();
    press(slider, { fraction: 0.25 }).release();
    expect(sent(onChange)).toEqual([-45]);
  });

  it("stays under the mouse after the mouse leaves an end", () => {
    const { slider, onChange } = renderSlider(-30);
    const drag = press(slider).to(1.2);
    expect(slider).toHaveAttribute("aria-valuenow", "0");
    drag.to(1.1);
    expect(sent(onChange)).toEqual([0]);
    drag.to(0.5).release();
    expect(sent(onChange)).toEqual([0, -30]);
  });

  it("takes focus when clicked or dragged", () => {
    const { slider } = renderSlider();
    press(slider).release();
    expect(slider).toHaveFocus();
    (document.activeElement as HTMLElement).blur();
    slide(slider, 0.1);
    expect(slider).toHaveFocus();
  });

  it("shows the focus ring after a click only once a key is pressed", () => {
    const { slider } = renderSlider();
    press(slider).release();
    expect(slider).toHaveAttribute("data-pointer-focused");
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(slider).not.toHaveAttribute("data-pointer-focused");
    press(slider).release();
    fireEvent.blur(slider);
    expect(slider).not.toHaveAttribute("data-pointer-focused");
  });

  it("steps with the arrow keys, and bigger steps with Shift, each change on its own", () => {
    const { slider, onChange } = renderSlider(-30);
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    fireEvent.keyDown(slider, { key: "ArrowUp" });
    fireEvent.keyDown(slider, { key: "ArrowLeft", shiftKey: true });
    fireEvent.keyDown(slider, { key: "ArrowDown", shiftKey: true });
    fireEvent.keyDown(slider, { key: "End" });
    fireEvent.keyDown(slider, { key: "Home" });
    // Each steps from the project's value, which Rust hasn't changed.
    expect(sent(onChange)).toEqual([-29.5, -29.5, -35, -35, 0, -60]);
    expect(gestures(onChange)).toEqual([undefined, undefined, undefined, undefined, undefined, undefined]);
  });

  it("sends nothing for a key at an end", () => {
    const { slider, onChange } = renderSlider(0);
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    fireEvent.keyDown(slider, { key: "End" });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("moves in fine steps with ⌥ held, and carries on from there when it's let go", () => {
    const { slider, onChange } = renderSlider(-30);
    // 100 pixels is 12 steps plain, and a tenth of that with ⌥.
    const drag = press(slider, { altKey: true }).by(100, { altKey: true });
    expect(sent(onChange)).toEqual([-30 + 0.5 * Math.round(12 * FINE_DRAG)]);
    drag.by(100);
    expect(sent(onChange).at(-1)).toBe(-29.5 + 6);
    drag.release();
    expect(new Set(gestures(onChange)).size).toBe(1);
  });

  it("goes fine mid-drag from where the thumb is", () => {
    const { slider, onChange } = renderSlider(-30);
    const drag = press(slider).to(0.75);
    expect(sent(onChange)).toEqual([-15]);
    // 30 pixels is 3.6 steps plain, and with ⌥ under half a step.
    drag.by(-30, { altKey: true });
    expect(sent(onChange)).toEqual([-15]);
    drag.by(-30, { altKey: true });
    expect(sent(onChange)).toEqual([-15, -15.5]);
    drag.release();
  });

  it("resets to its default on double-click", () => {
    const { slider, onChange, project } = renderSlider(-30, -12);
    fireEvent.doubleClick(slider);
    expect(onChange).toHaveBeenCalledExactlyOnceWith(-12, undefined);
    project(-12);
    fireEvent.doubleClick(slider);
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("resets a double-click off the thumb in the same gesture as the press's jump", () => {
    const { slider, onChange, project } = renderSlider(-30, -12);
    // A real double-click: press, release, press, release, then dblclick.
    press(slider, { fraction: 0.1 }).release();
    project(-54);
    press(slider, { fraction: 0.1 }).release();
    fireEvent.doubleClick(slider);
    const [jump, reset] = onChange.mock.calls;
    expect(onChange).toHaveBeenCalledTimes(2);
    expect(jump).toEqual([-54, expect.any(Number)]);
    // One gesture, so Rust undoes both as one step, back to -30.
    expect(reset).toEqual([-12, jump[1]]);
  });

  it("resets as its own step when the last drag was a while ago", () => {
    const { slider, onChange } = renderSlider(-30, -12);
    slide(slider, 0.25);
    const now = performance.now();
    vi.spyOn(performance, "now").mockReturnValue(now + 1000);
    fireEvent.doubleClick(slider);
    vi.restoreAllMocks();
    expect(onChange.mock.calls.at(-1)).toEqual([-12, undefined]);
  });

  it("marks its default on the rail", () => {
    const { slider } = renderSlider(-30, -12);
    const tick = slider.querySelector<HTMLElement>(".slider-default");
    expect(tick?.style.left).toBe("80%");
  });

  it("ends a drag the window loses where it is", () => {
    const { slider, onChange, project } = renderSlider(-30);
    press(slider).to(0.25);
    fireEvent.blur(window);
    project(-45);
    fireEvent.pointerMove(window, { clientX: 900 });
    expect(sent(onChange)).toEqual([-45]);
    expect(slider).toHaveAttribute("aria-valuenow", "-45");
  });
});
