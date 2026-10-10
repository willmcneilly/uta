import type { FieldsetHTMLAttributes } from "react";
import "./Group.css";

// A titled group of controls in a panel, such as the synth's Waveform,
// Filter and Envelope.

interface Props extends Omit<FieldsetHTMLAttributes<HTMLFieldSetElement>, "title"> {
  /** The section title, which names the group. */
  title: string;
  /** The title's ID, for a group that points `aria-labelledby` at it. */
  titleId?: string;
}

/** A group of controls under a section title. */
export function Group({ title, titleId, className = "", children, ...fieldset }: Props) {
  return (
    <fieldset className={`group ${className}`.trim()} {...fieldset}>
      <legend id={titleId} className="group-title">
        {title}
      </legend>
      {children}
    </fieldset>
  );
}
