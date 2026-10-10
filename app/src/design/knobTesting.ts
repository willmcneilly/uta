// Drives a knob with the pointer in tests. A knob is dragged up and down:
// KNOB_TRAVEL pixels upwards take it from one end to the other.

import { fireEvent } from "@testing-library/react";
import { KNOB_SWEEP, KNOB_TRAVEL } from "./Knob";
import type { Keys } from "./numberControlBehaviour";

/** How far round `knob`'s indicator is drawn, from 0 at the bottom left to 1 at the bottom right. */
export function knobAt(knob: HTMLElement): number {
  const indicator = knob.querySelector(".knob-indicator");
  if (!indicator) throw new Error("not a knob");
  const angle = Number(/rotate\((-?[\d.e-]+)/.exec(indicator.getAttribute("transform") ?? "")?.[1]);
  return angle / KNOB_SWEEP + 0.5;
}

/** Where the pointer is for `fraction` of the way round: higher up for more. */
const yOf = (fraction: number) => (1 - fraction) * KNOB_TRAVEL;

export interface KnobDrag {
  /** Moves the pointer to where `fraction` of the way round is, from where the knob was pressed. */
  to: (fraction: number, keys?: Keys) => KnobDrag;
  /** Moves the pointer `pixels` upwards from where it is. */
  by: (pixels: number, keys?: Keys) => KnobDrag;
  release: () => void;
}

/**
 * Presses on `knob`, with the pointer level with where its value is drawn,
 * so `to` takes it to a fraction of the way round, or `offset` pixels above
 * that. Returns the drag, to move and release.
 */
export function pressKnob(knob: HTMLElement, options: { offset?: number } & Keys = {}): KnobDrag {
  const { offset = 0, ...keys } = options;
  let y = yOf(knobAt(knob)) - offset;
  fireEvent.pointerDown(knob, { button: 0, clientY: y, ...keys });
  const drag: KnobDrag = {
    to: (fraction, keys = {}) => {
      y = yOf(fraction);
      fireEvent.pointerMove(window, { clientY: y, ...keys });
      return drag;
    },
    by: (pixels, keys = {}) => {
      y -= pixels;
      fireEvent.pointerMove(window, { clientY: y, ...keys });
      return drag;
    },
    release: () => {
      fireEvent.pointerUp(window);
    },
  };
  return drag;
}

/** Grabs `knob`, drags it through each fraction of the way round in turn, and lets go. */
export function slideKnob(knob: HTMLElement, ...fractions: number[]): void {
  const drag = pressKnob(knob);
  for (const fraction of fractions) drag.to(fraction);
  drag.release();
}
