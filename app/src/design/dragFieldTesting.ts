// Drives a drag field with the pointer, and types into it, in tests. jsdom
// has no layout, so a drag is told in pixels up from the press.

import { fireEvent } from "@testing-library/react";
import { PIXELS_PER_STEP } from "./DragField";
import type { Keys } from "./numberControlBehaviour";

// Where presses land, in pixels from the top of the window.
const PRESS_Y = 500;

export interface FieldDrag {
  /** Moves the pointer `pixels` up (down if negative) from where it is. */
  by: (pixels: number, keys?: Keys) => FieldDrag;
  /** Moves the pointer up far enough to raise it `steps` steps (down if negative), at the plain rate. */
  up: (steps: number, keys?: Keys) => FieldDrag;
  release: () => void;
}

/** Presses on `field`, `offset` pixels above the middle, with `keys` held. */
export function press(field: HTMLElement, options: { offset?: number } & Keys = {}): FieldDrag {
  const { offset = 0, ...keys } = options;
  let y = PRESS_Y - offset;
  fireEvent.pointerDown(field, { button: 0, clientX: 100, clientY: y, ...keys });
  const drag: FieldDrag = {
    by: (pixels, keys = {}) => {
      y -= pixels;
      fireEvent.pointerMove(window, { clientX: 100, clientY: y, ...keys });
      return drag;
    },
    up: (steps, keys) => drag.by(steps * PIXELS_PER_STEP, keys),
    release: () => {
      fireEvent.pointerUp(window);
    },
  };
  return drag;
}

/** Clicks `field` without dragging, which opens it for typing, and returns the box. */
export function openForTyping(field: HTMLElement): HTMLInputElement {
  press(field).release();
  const box = field.parentElement?.querySelector<HTMLInputElement>("input");
  if (!box) throw new Error("the drag field didn't open for typing");
  return box;
}

/** Clicks `field`, types `text` over its value, and presses Return. */
export function typeInto(field: HTMLElement, text: string): void {
  const box = openForTyping(field);
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
}
