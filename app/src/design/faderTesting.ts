// Drives a fader with the pointer in tests. jsdom has no layout, so each
// fader's rail is given a length here: RAIL_WIDTH pixels, from x = 0 along
// a fader, or up from y = 0 on one stood on end (so y runs negative).

import { fireEvent } from "@testing-library/react";
import type { Keys } from "./numberControlBehaviour";

/** How wide a fader's rail is in tests, in CSS pixels: a pixel is a thousandth of the way. */
export const RAIL_WIDTH = 1000;

const vertical = (fader: HTMLElement) => fader.getAttribute("aria-orientation") === "vertical";

/** `fader`'s rail, given RAIL_WIDTH along it. */
function measure(fader: HTMLElement): HTMLElement {
  const rail = fader.querySelector<HTMLElement>(".fader-rail");
  if (!rail) throw new Error("not a fader");
  const [width, height] = vertical(fader) ? [0, RAIL_WIDTH] : [RAIL_WIDTH, 0];
  rail.getBoundingClientRect = () =>
    ({ x: 0, y: -height, left: 0, top: -height, right: width, bottom: 0, width, height }) as DOMRect;
  return rail;
}

/** Where `fader`'s thumb is drawn, in pixels from the rail's left end, or up from its bottom. */
export function thumbX(fader: HTMLElement): number {
  const thumb = measure(fader).querySelector<HTMLElement>(".fader-thumb");
  const along = vertical(fader) ? thumb?.style.bottom : thumb?.style.left;
  return (parseFloat(along ?? "0") / 100) * RAIL_WIDTH;
}

/** The pointer `x` pixels along `fader`'s rail. */
const at = (fader: HTMLElement, x: number) => (vertical(fader) ? { clientY: -x } : { clientX: x });

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
  fireEvent.pointerDown(fader, { button: 0, ...at(fader, x), ...keys });
  const drag: Drag = {
    to: (fraction, keys = {}) => {
      x = fraction * RAIL_WIDTH;
      fireEvent.pointerMove(window, { ...at(fader, x), ...keys });
      return drag;
    },
    by: (pixels, keys = {}) => {
      x += pixels;
      fireEvent.pointerMove(window, { ...at(fader, x), ...keys });
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
