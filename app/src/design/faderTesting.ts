// Drives a fader with the pointer in tests. jsdom has no layout, so each
// fader's rail is given a width here: RAIL_WIDTH pixels, from x = 0.

import { fireEvent } from "@testing-library/react";

/** How wide a fader's rail is in tests, in CSS pixels: a pixel is a thousandth of the way. */
export const RAIL_WIDTH = 1000;

/** `fader`'s rail, given RAIL_WIDTH. */
function measure(fader: HTMLElement): HTMLElement {
  const rail = fader.querySelector<HTMLElement>(".fader-rail");
  if (!rail) throw new Error("not a fader");
  rail.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: RAIL_WIDTH, bottom: 0, width: RAIL_WIDTH, height: 0 }) as DOMRect;
  return rail;
}

/** Where `fader`'s thumb is drawn, in pixels from the rail's left end. */
export function thumbX(fader: HTMLElement): number {
  const thumb = measure(fader).querySelector<HTMLElement>(".fader-thumb");
  return (parseFloat(thumb?.style.left ?? "0") / 100) * RAIL_WIDTH;
}

/** Keys held during a press or a move. */
export interface Keys {
  shiftKey?: boolean;
  altKey?: boolean;
}

export interface Drag {
  /** Moves the pointer to `fraction` of the way along the rail. */
  to: (fraction: number, keys?: Keys) => Drag;
  /** Moves the pointer `pixels` from where it is. */
  by: (pixels: number, keys?: Keys) => Drag;
  release: () => void;
}

/**
 * Presses on `fader`: on its thumb, unless `fraction` says where along the
 * rail. Returns the drag, to move and release.
 */
export function press(fader: HTMLElement, options: { fraction?: number } & Keys = {}): Drag {
  const { fraction, ...keys } = options;
  let x = fraction === undefined ? thumbX(fader) : fraction * RAIL_WIDTH;
  measure(fader);
  fireEvent.pointerDown(fader, { button: 0, clientX: x, ...keys });
  const drag: Drag = {
    to: (fraction, keys = {}) => {
      x = fraction * RAIL_WIDTH;
      fireEvent.pointerMove(window, { clientX: x, ...keys });
      return drag;
    },
    by: (pixels, keys = {}) => {
      x += pixels;
      fireEvent.pointerMove(window, { clientX: x, ...keys });
      return drag;
    },
    release: () => {
      fireEvent.pointerUp(window);
    },
  };
  return drag;
}

/** Grabs `fader`'s thumb, drags it through each fraction of the way along in turn, and lets go. */
export function slide(fader: HTMLElement, ...fractions: number[]): void {
  const drag = press(fader);
  for (const fraction of fractions) drag.to(fraction);
  drag.release();
}
