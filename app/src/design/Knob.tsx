import { useId } from "react";
import { type NumberControlOptions, useNumberControl } from "./useNumberControl";
import "./Knob.css";

// Every knob in the app: a setting in a compact group that stands on its own,
// such as pan or the synth's cutoff. What it does when you drag it, press
// keys or reset it is `useNumberControl`'s, shared with every control that
// sets a number; this draws it. It's dragged up and down, never in a circle.

/** How far a drag goes, in CSS pixels, to take a knob from one end to the other. */
export const KNOB_TRAVEL = 200;

/** How far round a knob turns from one end to the other, in degrees. */
export const KNOB_SWEEP = 270;

/** The dial's size, in CSS pixels, and where its centre is. */
const SIZE = 32;
const CENTRE = SIZE / 2;
/** The range and value arcs' radius. */
const ARC = 11;
/** Where the indicator starts, from the centre. */
const HUB = 4;
/** The tick at the default, outside the arc. */
const TICK = [13, 15.5] as const;

interface Props extends NumberControlOptions {
  /** The visible label, and its name unless `ariaLabel` gives one. */
  label: string;
  /** What assistive tech calls it, when that's more than the label ("Synth 1 pan"). */
  ariaLabel?: string;
  /** Leaves the label out, where the place around it says what it is. */
  hideLabel?: boolean;
  /** Fills the value from the middle of the range, for a centred setting such as pan. */
  centred?: boolean;
  /** The knob's class, for its layout in the area around it. */
  className?: string;
}

/** The angle at `fraction` of the way round, in degrees clockwise from the top. */
function knobAngle(fraction: number): number {
  return (fraction - 0.5) * KNOB_SWEEP;
}

/** The point on the circle of `radius` at `angle` degrees clockwise from the top. */
function point(angle: number, radius: number): string {
  const radians = (angle * Math.PI) / 180;
  const x = CENTRE + radius * Math.sin(radians);
  const y = CENTRE - radius * Math.cos(radians);
  return `${x.toFixed(3)},${y.toFixed(3)}`;
}

/** An arc of the dial from angle `from` to `to`, clockwise, or null when they meet. */
function arc(from: number, to: number): string | null {
  const [start, end] = from <= to ? [from, to] : [to, from];
  if (end - start < 0.5) return null;
  const large = end - start > 180 ? 1 : 0;
  return `M${point(start, ARC)} A${ARC},${ARC} 0 ${large} 1 ${point(end, ARC)}`;
}

/**
 * A label, the knob and its value underneath. Drag it up or down from
 * wherever you press, or click it and use the arrow keys (Shift for bigger
 * steps). Shift-drag moves it in fine steps. Double-click, ⌥-click, or
 * Delete resets it to its default. provisional: D-19
 */
export function Knob({
  label,
  ariaLabel,
  hideLabel = false,
  centred = false,
  className = "",
  ...options
}: Props) {
  const labelId = useId();
  const { position, text, props } = useNumberControl(options, () => KNOB_TRAVEL, {
    direction: "vertical",
  });
  const { scale, defaultValue } = options;
  const angle = knobAngle(position / scale.steps);
  const value = arc(knobAngle(centred ? 0.5 : 0), angle);
  const tick = knobAngle(scale.toPosition(defaultValue) / scale.steps);

  return (
    <div className={`knob ${className}`.trim()}>
      {hideLabel ? null : (
        <span className="knob-label" id={labelId}>
          {label}
        </span>
      )}
      <div
        className="knob-control"
        aria-label={ariaLabel ?? (hideLabel ? label : undefined)}
        aria-labelledby={ariaLabel || hideLabel ? undefined : labelId}
        aria-orientation="vertical"
        {...props}
      >
        <svg
          className="knob-dial"
          viewBox={`0 0 ${SIZE} ${SIZE}`}
          width={SIZE}
          height={SIZE}
          aria-hidden="true"
        >
          <path className="knob-range" d={arc(knobAngle(0), knobAngle(1)) ?? undefined} />
          <line
            className="knob-default"
            x1={CENTRE}
            y1={CENTRE - TICK[0]}
            x2={CENTRE}
            y2={CENTRE - TICK[1]}
            transform={`rotate(${tick} ${CENTRE} ${CENTRE})`}
          />
          {value ? <path className="knob-value" d={value} /> : null}
          <line
            className="knob-indicator"
            x1={CENTRE}
            y1={CENTRE - HUB}
            x2={CENTRE}
            y2={CENTRE - ARC}
            transform={`rotate(${angle} ${CENTRE} ${CENTRE})`}
          />
        </svg>
        <span className="knob-value-text">{text}</span>
      </div>
    </div>
  );
}
