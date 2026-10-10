import type { ButtonHTMLAttributes } from "react";
import "./Key.css";

// Every key in the app that does one thing when you press it: Play, Stop,
// Loop, + Add track, the benchmark's buttons. A faint outline and ink words,
// with a small drawn mark where one helps.

/** The marks a key can draw before its words. */
export type KeyMark = "play" | "stop" | "loop";

interface Props extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type"> {
  /** A small drawing before the words. */
  mark?: KeyMark;
  /** What the key starts is playing now: its mark turns the live ink. */
  playing?: boolean;
  /** A key that stays down while it's on, such as Loop. Leave it out for one that doesn't. */
  pressed?: boolean;
  /** The one that goes ahead, of a few side by side: outlined in ink. */
  primary?: boolean;
}

/** A key. It's a button, and a toggle button when `pressed` is given. */
export function Key({ mark, playing, pressed, primary, className = "", ...button }: Props) {
  return (
    <button
      type="button"
      className={`key${primary ? " key-primary" : ""} ${className}`.trim()}
      data-mark={mark}
      data-playing={playing || undefined}
      aria-pressed={pressed}
      {...button}
    />
  );
}
