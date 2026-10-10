import type { SelectHTMLAttributes } from "react";
import "./Menu.css";

// Every menu in the app: snap in the timeline and the piano roll, and the
// buffer size. The system's own pop-up list, opened from a flat key in ink
// with a small chevron in place of the native arrow.

type Props = SelectHTMLAttributes<HTMLSelectElement>;

/** A menu: a `select`, with its `option`s as children. */
export function Menu({ className = "", ...select }: Props) {
  return (
    <span className={`menu ${className}`.trim()}>
      <select {...select} />
    </span>
  );
}
