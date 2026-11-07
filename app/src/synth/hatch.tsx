import { type ReactNode, useId } from "react";
import { readHatchGap } from "../design/readTokens";
import { useTokenVersion } from "../design/tokenChanges";

// The hatching under a drawing's curve, shared by the synth's drawings and
// the Reverb's.

/**
 * The hatching pattern: diagonal lines in the hatch ink, every `hatch-gap`
 * (DESIGN.md, Shapes). Returns its `fill` and its definition.
 */
export function useHatch(): [string, ReactNode] {
  const id = `hatch-${useId().replace(/[^a-zA-Z0-9-]/g, "")}`;
  // Re-rendered when the tokens change, so the gap is read again then.
  useTokenVersion();
  const gap = readHatchGap(document.documentElement);
  const pattern = (
    <defs>
      <pattern
        id={id}
        width={gap}
        height={gap}
        patternUnits="userSpaceOnUse"
        patternTransform="rotate(45)"
      >
        <line className="hatch" x1={gap / 2} y1={0} x2={gap / 2} y2={gap} />
      </pattern>
    </defs>
  );
  return [`url(#${id})`, pattern];
}
