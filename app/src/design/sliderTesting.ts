// Drives a slider with the pointer in tests. jsdom has no layout, so each
// slider's rail is given a width here: RAIL_WIDTH pixels, from x = 0.

import { fireEvent } from "@testing-library/react";

/** How wide a slider's rail is in tests, in CSS pixels: a pixel is a thousandth of the way. */
export const RAIL_WIDTH = 1000;

/** `slider`'s rail, given RAIL_WIDTH. */
function measure(slider: HTMLElement): HTMLElement {
  const rail = slider.querySelector<HTMLElement>(".slider-rail");
  if (!rail) throw new Error("not a slider");
  rail.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: RAIL_WIDTH, bottom: 0, width: RAIL_WIDTH, height: 0 }) as DOMRect;
  return rail;
}

/** Where `slider`'s thumb is drawn, in pixels from the rail's left end. */
export function thumbX(slider: HTMLElement): number {
  const thumb = measure(slider).querySelector<HTMLElement>(".slider-thumb");
  return (parseFloat(thumb?.style.left ?? "0") / 100) * RAIL_WIDTH;
}

export interface Drag {
  /** Moves the pointer to `fraction` of the way along the rail. */
  to: (fraction: number, options?: { altKey?: boolean }) => Drag;
  /** Moves the pointer `pixels` from where it is. */
  by: (pixels: number, options?: { altKey?: boolean }) => Drag;
  release: () => void;
}

/**
 * Presses on `slider`: on its thumb, unless `fraction` says where along the
 * rail. Returns the drag, to move and release.
 */
export function press(slider: HTMLElement, options: { fraction?: number; altKey?: boolean } = {}): Drag {
  let x = options.fraction === undefined ? thumbX(slider) : options.fraction * RAIL_WIDTH;
  measure(slider);
  fireEvent.pointerDown(slider, { button: 0, clientX: x, altKey: options.altKey ?? false });
  const drag: Drag = {
    to: (fraction, { altKey = false } = {}) => {
      x = fraction * RAIL_WIDTH;
      fireEvent.pointerMove(window, { clientX: x, altKey });
      return drag;
    },
    by: (pixels, { altKey = false } = {}) => {
      x += pixels;
      fireEvent.pointerMove(window, { clientX: x, altKey });
      return drag;
    },
    release: () => {
      fireEvent.pointerUp(window);
    },
  };
  return drag;
}

/** Grabs `slider`'s thumb, drags it through each fraction of the way along in turn, and lets go. */
export function slide(slider: HTMLElement, ...fractions: number[]): void {
  const drag = press(slider);
  for (const fraction of fractions) drag.to(fraction);
  drag.release();
}
