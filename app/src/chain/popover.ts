import { type KeyboardEvent, type RefObject, useEffect, useLayoutEffect, useRef } from "react";

// A menu that opens from a key on the row: the + menu and the preset menus.
// Opened, it's placed against the window, not the row, so the scrolling row
// never clips it wherever its key is: below the key and to its right, or to
// its left if the window's edge is in the way. It's placed before it's
// drawn, so it never shows in the wrong spot. Its first item takes focus,
// the arrow keys move through its items, and a click anywhere else, Escape,
// or scrolling or resizing what it was placed against closes it.
// provisional: D-36

/** The least room a menu leaves to the window's edges, in CSS pixels. */
const MENU_MARGIN = 8;

/** The parts of a menu and its key, which {@link usePopover} gives back to be put on them. */
export interface Popover {
  /** Everything that's part of it: the key and the menu. A click outside it closes the menu. */
  area: RefObject<HTMLDivElement | null>;
  /** The key that opens it, which it's placed against and gives focus back to on Escape. */
  key: RefObject<HTMLButtonElement | null>;
  menu: RefObject<HTMLDivElement | null>;
}

/** The menu's items, in order. */
function itemsOf(area: RefObject<HTMLElement | null>): HTMLElement[] {
  return Array.from(area.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? []);
}

/**
 * Places, focuses and closes an open menu, through the refs it gives back
 * for the menu's area, key and menu. `drop` is how far below the top of its
 * key it opens, in CSS pixels.
 */
export function usePopover(open: boolean, close: () => void, drop: number): Popover {
  const area = useRef<HTMLDivElement>(null);
  const key = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const at = key.current?.getBoundingClientRect();
    const box = menu.current;
    if (!open || !at || !box) return;
    const width = box.offsetWidth;
    const right = at.right + width <= window.innerWidth - MENU_MARGIN;
    box.style.top = `${at.top + drop}px`;
    box.style.left = `${right ? at.right : Math.max(MENU_MARGIN, at.left - width)}px`;
  }, [open, drop]);

  useEffect(() => {
    if (!open) return;
    itemsOf(area)[0]?.focus();
    const moved = () => close();
    window.addEventListener("scroll", moved, true);
    window.addEventListener("resize", moved);
    const away = (event: globalThis.PointerEvent) => {
      if (!area.current?.contains(event.target as Node)) close();
    };
    const escape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      close();
      key.current?.focus();
    };
    window.addEventListener("pointerdown", away);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", away);
      window.removeEventListener("keydown", escape);
      window.removeEventListener("scroll", moved, true);
      window.removeEventListener("resize", moved);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `close` is a new function on every render; only opening and closing matter
  }, [open]);
  return { area, key, menu };
}

/** Up and down move through a menu's items, round from the ends; Home and End go to them. */
export function moveFocus(area: RefObject<HTMLElement | null>, event: KeyboardEvent<HTMLElement>): void {
  const all = itemsOf(area);
  const now = all.indexOf(document.activeElement as HTMLElement);
  const next = {
    ArrowDown: (now + 1) % all.length,
    ArrowUp: (now - 1 + all.length) % all.length,
    Home: 0,
    End: all.length - 1,
  }[event.key];
  if (next === undefined) return;
  event.preventDefault();
  all[next]?.focus();
}
