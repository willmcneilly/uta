import { type KeyboardEvent, type ReactNode, useId, useRef } from "react";
import "./SegmentedChoice.css";

// Every segmented choice in the app: five or fewer named options, all
// visible, such as the waveform. To assistive tech it's a radio group.

export interface Option<T extends string> {
  value: T;
  label: string;
  /** Drawn above the label, such as one cycle of a wave. Hidden from assistive tech. */
  drawing?: ReactNode;
}

interface Props<T extends string> {
  /** The group's visible name. */
  legend: string;
  /** The radios' shared name. */
  name: string;
  options: Option<T>[];
  /** The chosen option, as Rust last sent it. */
  value: T;
  onChange: (value: T) => void;
  /** The group's class, for its layout in the area around it. */
  className?: string;
}

const ARROW_STEPS: Record<string, number> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

/**
 * An outlined row of options, with the chosen one in ink. Click one to
 * choose it, or use the arrow keys, which wrap round at the ends.
 *
 * WebKit on macOS doesn't focus a radio button when it's clicked, and leaves
 * radios out of the Tab order, so the group focuses them itself, gives the
 * chosen one an explicit tabIndex, and handles the arrow keys itself.
 * provisional: D-19
 */
export function SegmentedChoice<T extends string>({
  legend,
  name,
  options,
  value,
  onChange,
  className = "",
}: Props<T>) {
  const legendId = useId();
  const radios = useRef(new Map<T, HTMLInputElement>());

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>, index: number) => {
    const step = ARROW_STEPS[event.key];
    if (step === undefined) return;
    event.preventDefault();
    const next = options[(index + step + options.length) % options.length].value;
    radios.current.get(next)?.focus();
    if (next !== value) onChange(next);
  };

  return (
    <fieldset
      className={`segmented ${className}`.trim()}
      role="radiogroup"
      aria-labelledby={legendId}
    >
      <legend id={legendId}>{legend}</legend>
      <div className="segmented-row">
        {options.map((option, index) => (
          <label key={option.value}>
            <input
              ref={(radio) => {
                if (radio) radios.current.set(option.value, radio);
                else radios.current.delete(option.value);
              }}
              type="radio"
              name={name}
              value={option.value}
              checked={value === option.value}
              // An explicit tabIndex puts it in WebKit's Tab order.
              tabIndex={value === option.value ? 0 : -1}
              onClick={(event) => event.currentTarget.focus()}
              onKeyDown={(event) => onKeyDown(event, index)}
              onChange={() => onChange(option.value)}
            />
            {option.drawing ? (
              <span className="segmented-drawing" aria-hidden="true">
                {option.drawing}
              </span>
            ) : null}
            {option.label}
          </label>
        ))}
      </div>
    </fieldset>
  );
}
