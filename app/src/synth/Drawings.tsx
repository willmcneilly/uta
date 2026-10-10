import { type ReactNode, useId } from "react";
import { readHatchGap } from "../design/readTokens";
import { useTokenVersion } from "../design/tokenChanges";
import { formatAmount, formatHz, formatLevel, formatSeconds } from "../synthScale";
import { type EnvelopeTimes, type StageName, envelopeShape } from "./envelopeShape";
import { type Point, filterCurve, xOfFrequency, yOfGain } from "./filterCurve";

// The synth panel's drawings of what the filter and the envelope do: read
// only, so the sliders stay the controls. They're SVG, coloured and weighted
// by the tokens through CSS (SynthPanel.css), so they follow the theme and
// the tuning panel. Only the hatching's spacing is read here, because an SVG
// pattern's size is an attribute, not a style.

/**
 * Each drawing's size in CSS pixels, without its labels: wide enough to read
 * three decades of frequency, short enough to sit above its sliders.
 * provisional: D-10
 */
export const DRAWING_WIDTH = 288;
export const DRAWING_HEIGHT = 88;
/** Room under the filter drawing for its frequency labels. */
const LABEL_ROOM = 14;
/** The frequencies the filter drawing marks, with their labels. */
const MARKED_HZ: [number, string][] = [
  [100, "100"],
  [1_000, "1k"],
  [10_000, "10k"],
];

/** An SVG path through `points`. */
function line(points: Point[]): string {
  return points
    .map(({ x, y }, i) => `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`)
    .join(" ");
}

/** The area under `points`, down to the bottom edge, to hatch. */
function area(points: Point[], height: number): string {
  const first = points[0];
  const last = points[points.length - 1];
  return `${line(points)} L${last.x.toFixed(2)},${height} L${first.x.toFixed(2)},${height} Z`;
}

/**
 * The hatching pattern: diagonal lines in the hatch ink, every `hatch-gap`
 * (DESIGN.md, Shapes). Returns its `fill` and its definition.
 */
function useHatch(): [string, ReactNode] {
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

interface FilterProps {
  cutoffHz: number;
  resonance: number;
  sampleRate: number;
  /** Mark the cutoff, while its slider or the resonance's is dragged. */
  marked: boolean;
}

/** The filter's response: how much of each frequency it lets through. */
export function FilterDrawing({ cutoffHz, resonance, sampleRate, marked }: FilterProps) {
  const [hatch, pattern] = useHatch();
  const [width, height] = [DRAWING_WIDTH, DRAWING_HEIGHT];
  const curve = filterCurve(cutoffHz, resonance, sampleRate, width, height);
  const cutoffX = xOfFrequency(cutoffHz, width);
  const unity = yOfGain(0, height);
  return (
    <svg
      className="drawing"
      role="img"
      aria-label={`Filter response: low-pass at ${formatHz(cutoffHz)}, resonance ${formatAmount(
        resonance,
      )}`}
      width={width}
      height={height + LABEL_ROOM}
      data-testid="filter-drawing"
    >
      {pattern}
      <rect className="frame" x={0} y={0} width={width} height={height} />
      {MARKED_HZ.map(([hz, label]) => {
        const x = xOfFrequency(hz, width);
        return (
          <g key={hz}>
            <line className="grid" x1={x} y1={0} x2={x} y2={height} />
            <text className="axis" x={x} y={height + LABEL_ROOM - 2} textAnchor="middle">
              {label}
            </text>
          </g>
        );
      })}
      <line className="grid" x1={0} y1={unity} x2={width} y2={unity} />
      <path className="under" d={area(curve, height)} fill={hatch} />
      {marked && <line className="dimension" x1={cutoffX} y1={0} x2={cutoffX} y2={height} />}
      <path className="curve" d={line(curve)} />
    </svg>
  );
}

interface EnvelopeProps {
  times: EnvelopeTimes;
  /** The stage whose slider is being dragged, to mark. */
  marked: StageName | null;
}

/** The envelope: a note's level from the key going down to the end of its release. */
export function EnvelopeDrawing({ times, marked }: EnvelopeProps) {
  const [hatch, pattern] = useHatch();
  const [width, height] = [DRAWING_WIDTH, DRAWING_HEIGHT];
  const { stages, points } = envelopeShape(times, width, height);
  const stage = stages.find(({ name }) => name === marked);
  const sustainY = (1 - times.sustain) * height;
  const { attackSeconds, decaySeconds, sustain, releaseSeconds } = times;
  return (
    <svg
      className="drawing"
      role="img"
      aria-label={[
        `Envelope: attack ${formatSeconds(attackSeconds)}`,
        `decay ${formatSeconds(decaySeconds)}`,
        `sustain ${formatLevel(sustain)}`,
        `release ${formatSeconds(releaseSeconds)}`,
      ].join(", ")}
      width={width}
      height={height}
      data-testid="envelope-drawing"
    >
      {pattern}
      <rect className="frame" x={0} y={0} width={width} height={height} />
      {stages.slice(1).map(({ name, start }) => (
        <line key={name} className="grid" x1={start} y1={0} x2={start} y2={height} />
      ))}
      <path className="under" d={area(points, height)} fill={hatch} />
      {stage?.name === "sustain" ? (
        <line className="dimension" x1={0} y1={sustainY} x2={width} y2={sustainY} />
      ) : (
        stage &&
        [stage.start, stage.end].map((x) => (
          <line key={x} className="dimension" x1={x} y1={0} x2={x} y2={height} />
        ))
      )}
      <path className="curve" d={line(points)} />
    </svg>
  );
}
