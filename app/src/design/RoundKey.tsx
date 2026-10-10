import type { ButtonHTMLAttributes } from "react";
import "./RoundKey.css";

// A small round key that stays down while it's on, marked with a letter:
// a track's mute and solo.

interface Props extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type" | "children"> {
  /** The letter drawn in it, such as "M". Name it for assistive tech with `aria-label`. */
  letter: string;
  /** Whether it's on. */
  pressed: boolean;
}

/** A round toggle key. */
export function RoundKey({ letter, pressed, className = "", ...button }: Props) {
  return (
    <button
      type="button"
      className={`round-key ${className}`.trim()}
      aria-pressed={pressed}
      {...button}
    >
      <span>{letter}</span>
    </button>
  );
}
