import { useId, useRef } from "react";
import { type NumberControlOptions, useNumberControl } from "./useNumberControl";
import "./Fader.css";

// Every fader in the app: a control that slides along a line, for values
// compared side by side, such as volume. What it does when you drag it,
// press keys or reset it is `useNumberControl`'s, shared with every control
// that sets a number; this draws it.

interface Props extends NumberControlOptions {
  /** The visible label. */
  label: string;
  /** What assistive tech calls it, when that's more than the label ("Synth 1 volume"). */
  ariaLabel?: string;
  /** The row's class, for its layout in the area around it. */
  className?: string;
  /**
   * Which way it runs. Stood on end, it's dragged up and down, and takes
   * the height its place gives it: the drum panel's Levels (provisional: D-26).
   */
  orientation?: "horizontal" | "vertical";
}

/**
 * A label, the fader and its value. Drag it from wherever you press, or
 * click it and use the arrow keys (Shift for bigger steps). Shift-drag moves
 * it in fine steps. Double-click, ⌥-click, or Delete resets it to its default.
 */
export function Fader({
  label,
  ariaLabel,
  className = "",
  orientation = "horizontal",
  ...options
}: Props) {
  const labelId = useId();
  const rail = useRef<HTMLSpanElement>(null);
  const vertical = orientation === "vertical";
  const { position, text, props } = useNumberControl(
    options,
    () => {
      const box = rail.current?.getBoundingClientRect();
      return box ? (vertical ? box.height : box.width) : null;
    },
    { direction: orientation },
  );
  const { scale, defaultValue } = options;
  // Along the rail: from the left, or up from the bottom.
  const along = (at: number) => ({ [vertical ? "bottom" : "left"]: `${(at / scale.steps) * 100}%` });

  return (
    <div className={`fader-row ${className}`.trim()}>
      <span id={labelId}>{label}</span>
      <div
        className="fader"
        aria-label={ariaLabel}
        aria-labelledby={ariaLabel ? undefined : labelId}
        aria-orientation={orientation}
        {...props}
      >
        <span className="fader-rail" ref={rail}>
          <span className="fader-default" style={along(scale.toPosition(defaultValue))} />
          <span className="fader-thumb" style={along(position)} />
        </span>
      </div>
      <output>{text}</output>
    </div>
  );
}
