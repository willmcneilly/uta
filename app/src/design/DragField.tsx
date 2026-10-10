import {
  type KeyboardEvent,
  type MouseEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { parseTyped, type Units } from "./parseTyped";
import { type NumberControlOptions, useNumberControl } from "./useNumberControl";
import "./DragField.css";

// Every drag field in the app: a number you drag up or down, or click and
// type, for a crowded place where the exact number matters, such as tempo.
// What it does when you drag it, press keys or reset it is
// `useNumberControl`'s, shared with every control that sets a number; this
// draws it, and adds typing.

/** How many pixels of drag move a drag field one step, unless it says otherwise. */
export const PIXELS_PER_STEP = 2;

interface Props extends NumberControlOptions {
  /** The visible label. */
  label: string;
  /** The units it understands when typed, with what each is in its own unit. */
  units: Units;
  /** How many pixels of drag move it one step. */
  pixelsPerStep?: number;
  /** The row's class, for its layout in the area around it. */
  className?: string;
}

/**
 * A label and the value with its unit. Drag it up or down from wherever you
 * press (Shift for fine steps), or click it and use the arrow keys. A click
 * without dragging opens it for typing: Return sets what you typed, as one
 * undo step, and Escape leaves it as it was. Double-click, ⌥-click, or
 * Delete resets it to its default.
 */
export function DragField({
  label,
  units,
  pixelsPerStep = PIXELS_PER_STEP,
  className = "",
  ...options
}: Props) {
  const labelId = useId();
  const field = useRef<HTMLSpanElement>(null);
  const box = useRef<HTMLInputElement>(null);
  // What's in the box while it's open for typing, and null when it's shut.
  const [typing, setTyping] = useState<string | null>(null);
  // Whether it's open, at once: Return shuts it and moves focus, and the
  // box's blur that follows mustn't set it a second time.
  const open = useRef(false);
  // Whether the box has been pressed since it opened, so the second press
  // of the double-click that opened it can be told from one in the box.
  const pressedInBox = useRef(false);
  const { value, scale, format, onChange } = options;
  const { text, props } = useNumberControl(options, () => scale.steps * pixelsPerStep, {
    direction: "vertical",
    onClick: () => {
      open.current = true;
      pressedInBox.current = false;
      setTyping(format(Math.min(scale.max, Math.max(scale.min, value))));
    },
  });

  // Opened by a click: ready to type over.
  const isOpen = typing !== null;
  useEffect(() => {
    if (!isOpen) return;
    box.current?.focus();
    box.current?.select();
  }, [isOpen]);

  const shut = () => {
    open.current = false;
    setTyping(null);
  };

  /** Sets what was typed, to the nearest step and limit, if it can be read. */
  const set = (typed: string) => {
    if (!open.current) return;
    shut();
    const read = parseTyped(typed, units);
    if (read === null || !Number.isFinite(read)) return;
    const next = scale.fromPosition(scale.toPosition(read));
    if (next !== value) onChange(next);
  };

  const onBoxKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      set(event.currentTarget.value);
      field.current?.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      shut();
      field.current?.focus();
    }
  };

  /** Shuts the box without setting it, and resets the field, which keeps focus. */
  const reset = (event: MouseEvent<HTMLElement>) => {
    shut();
    field.current?.focus();
    props.onDoubleClick(event);
  };

  return (
    <div className={`drag-field-row ${className}`.trim()}>
      <span id={labelId}>{label}</span>
      <span className="drag-field-box">
        <span
          className="drag-field"
          ref={field}
          aria-labelledby={labelId}
          aria-orientation="vertical"
          data-typing={isOpen || undefined}
          {...props}
          onDoubleClick={reset}
        >
          {text}
        </span>
        {isOpen && (
          <input
            className="drag-field-input"
            type="text"
            aria-labelledby={labelId}
            spellCheck={false}
            autoComplete="off"
            value={typing ?? ""}
            ref={box}
            onChange={(event) => setTyping(event.target.value)}
            onKeyDown={onBoxKeyDown}
            onBlur={(event) => set(event.currentTarget.value)}
            onMouseDown={(event) => {
              // The second press of a double-click on the field: it resets.
              if (!pressedInBox.current && event.detail >= 2) {
                event.preventDefault();
                reset(event);
              }
              pressedInBox.current = true;
            }}
          />
        )}
      </span>
    </div>
  );
}
