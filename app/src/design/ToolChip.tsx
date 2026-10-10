import type { ReactNode } from "react";
import "./ToolChip.css";

// The small tools over a canvas, such as snap and zoom: each a chip on its
// own sheet, holding a menu or flat keys.

interface Props {
  /**
   * A caption before what it holds, such as "Snap". It's for the eye only:
   * name the control inside with its own `aria-label`.
   */
  label?: string;
  /** For a chip of keys: "group", named with `aria-label`. */
  role?: "group";
  "aria-label"?: string;
  /** The chip's class, for its place in the area around it. */
  className?: string;
  children: ReactNode;
}

/** A chip of tools over a canvas. */
export function ToolChip({ label, className = "", children, ...group }: Props) {
  return (
    <div className={`tool-chip ${className}`.trim()} {...group}>
      {label !== undefined && <span aria-hidden="true">{label}</span>}
      {children}
    </div>
  );
}

interface ZoomProps {
  /** The caption between the keys, such as "Time". */
  label: string;
  /** What they zoom, for assistive tech: "Zoom in pitch". */
  what: string;
  onOut: () => void;
  onIn: () => void;
}

/** A pair of zoom keys, − and +, with a caption between them, for a chip. */
export function ZoomKeys({ label, what, onOut, onIn }: ZoomProps) {
  return (
    <>
      <button type="button" aria-label={`Zoom out ${what}`} onClick={onOut}>
        −
      </button>
      <span className="zoom-label">{label}</span>
      <button type="button" aria-label={`Zoom in ${what}`} onClick={onIn}>
        +
      </button>
    </>
  );
}
