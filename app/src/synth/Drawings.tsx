import { type ReactNode, useId } from "react";
import type { FilterMode, LfoShape, Waveform } from "../backend";
import { readHatchGap } from "../design/readTokens";
import { useTokenVersion } from "../design/tokenChanges";
import { type EnvelopeTimes, type StageName, envelopeShape } from "./envelopeShape";
import { type Point, filterCurve, reachHz, xOfFrequency, yOfGain } from "./filterCurve";
import { type Tuning, lfoPoints, oscillatorPoints } from "./waves";

// The synth panel's drawings of what its stages do: read only, so the
// controls stay the controls. They're SVG, coloured and weighted by the
// tokens through CSS (SynthPanel.css), so they follow the theme and the
// tuning panel. Only the hatching's spacing is read here, because an SVG
// pattern's size is an attribute, not a style. Each is drawn from the
// values its controls show, so it follows a drag before Rust replies.

/** Room under the filter drawing for its frequency labels. */
const LABEL_ROOM = 14;
/** The frequencies the filter drawing marks, with their labels. */
const MARKED_HZ: [number, string][] = [
  [100, "100"],
  [1_000, "1k"],
  [10_000, "10k"],
];
/** How far the dimension line from the cutoff to the envelope's reach sits from the top. */
const DIMENSION_Y = 5;
/** Half the height of a dimension line's ends, and the length of its arrowhead. */
const DIMENSION_END = 3;

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

/** A drawing's size. */
interface Size {
  width: number;
  height: number;
}

interface OscillatorProps extends Size {
  which: 1 | 2;
  wave: Waveform;
  /** Oscillator 1's pulse width. */
  pulseWidth: number;
  tuning: Tuning;
}

/**
 * An oscillator's wave over two of oscillator 1's cycles, so its width, its
 * pitch against oscillator 1 and sync's tear all show.
 */
export function OscillatorDrawing({ which, wave, pulseWidth, tuning, width, height }: OscillatorProps) {
  const points = oscillatorPoints(which, wave, pulseWidth, tuning, width, height);
  return (
    <svg
      className="drawing"
      role="img"
      aria-label={`Oscillator ${which}'s wave`}
      width={width}
      height={height}
      data-testid={`osc${which}-drawing`}
    >
      <line className="grid" x1={0} y1={height / 2} x2={width} y2={height / 2} />
      <line className="grid" x1={width / 2} y1={0} x2={width / 2} y2={height} />
      <path className="curve" d={line(points)} />
    </svg>
  );
}

interface FilterProps extends Size {
  cutoffHz: number;
  resonance: number;
  mode: FilterMode;
  /** How far the filter envelope moves the cutoff at its peak, in octaves. */
  envOctaves: number;
  sampleRate: number;
  /** Under the drawing, the frequencies it marks. */
  labels?: boolean;
  /** Mark the cutoff, while its knob or the resonance's is dragged. */
  marked?: boolean;
}

/**
 * The filter's response in its mode: how much of each frequency it lets
 * through. When the envelope moves the cutoff, the curve at the envelope's
 * peak is dashed, with a dimension line along the top from the cutoff to
 * it: how far each note opens (or closes) the filter.
 */
export function FilterDrawing({
  cutoffHz,
  resonance,
  mode,
  envOctaves,
  sampleRate,
  width,
  height,
  labels = false,
  marked = false,
}: FilterProps) {
  const [hatch, pattern] = useHatch();
  const curve = filterCurve(cutoffHz, resonance, mode, sampleRate, width, height);
  const reach = reachHz(cutoffHz, envOctaves);
  const cutoffX = xOfFrequency(cutoffHz, width);
  const reachX = xOfFrequency(reach, width);
  // Only where the reach is far enough from the cutoff to see.
  const reaches = Math.abs(reachX - cutoffX) > 2;
  const towards = reachX > cutoffX ? 1 : -1;
  const unity = yOfGain(0, height);
  return (
    <svg
      className="drawing"
      role="img"
      aria-label="Filter response"
      width={width}
      height={height + (labels ? LABEL_ROOM : 0)}
      data-testid="filter-drawing"
    >
      {pattern}
      <rect className="frame" x={0} y={0} width={width} height={height} />
      {labels &&
        MARKED_HZ.map(([hz, label]) => {
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
      {reaches && (
        <g data-testid="filter-reach">
          <path
            className="reach"
            d={line(filterCurve(reach, resonance, mode, sampleRate, width, height))}
          />
          <line
            className="span"
            x1={cutoffX}
            y1={DIMENSION_Y - DIMENSION_END}
            x2={cutoffX}
            y2={DIMENSION_Y + DIMENSION_END}
          />
          <line className="span" x1={cutoffX} y1={DIMENSION_Y} x2={reachX} y2={DIMENSION_Y} />
          <path
            className="span"
            d={`M${reachX - towards * DIMENSION_END * 1.7},${DIMENSION_Y - DIMENSION_END} L${reachX},${DIMENSION_Y} L${reachX - towards * DIMENSION_END * 1.7},${DIMENSION_Y + DIMENSION_END}`}
          />
        </g>
      )}
      {marked && <line className="dimension" x1={cutoffX} y1={0} x2={cutoffX} y2={height} />}
      <path className="curve" d={line(curve)} />
    </svg>
  );
}

interface EnvelopeProps extends Size {
  /** Which envelope: "Filter" or "Amp". */
  name: string;
  times: EnvelopeTimes;
  /** The stage whose knob is being dragged, to mark. */
  marked?: StageName | null;
}

/** An envelope: a note's level from the key going down to the end of its release. */
export function EnvelopeDrawing({ name, times, marked = null, width, height }: EnvelopeProps) {
  const [hatch, pattern] = useHatch();
  const { stages, points } = envelopeShape(times, width, height);
  const stage = stages.find((found) => found.name === marked);
  const sustainY = (1 - times.sustain) * height;
  return (
    <svg
      className="drawing"
      role="img"
      aria-label={`${name} envelope`}
      width={width}
      height={height}
      data-testid={`${name.toLowerCase()}-envelope-drawing`}
    >
      {pattern}
      <rect className="frame" x={0} y={0} width={width} height={height} />
      {stages.slice(1).map(({ name: stageName, start }) => (
        <line key={stageName} className="grid" x1={start} y1={0} x2={start} y2={height} />
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

interface LfoProps extends Size {
  shape: LfoShape;
  /** How many of its cycles fit in one bar. */
  cyclesPerBar: number;
  /** A line at each beat after the first. */
  beatsPerBar: number;
}

/**
 * The LFO's wave across one bar, with a line at each beat, so its rate
 * reads against the beat, synced or not.
 */
export function LfoDrawing({ shape, cyclesPerBar, beatsPerBar, width, height }: LfoProps) {
  const points = lfoPoints(shape, cyclesPerBar, width, height);
  // Past a few cycles a beat, a full-weight line is a solid block.
  const dense = shape === "sample_and_hold" || cyclesPerBar > 24;
  return (
    <svg
      className="drawing"
      role="img"
      aria-label="LFO over one bar"
      width={width}
      height={height}
      data-testid="lfo-drawing"
    >
      <rect className="frame" x={0} y={0} width={width} height={height} />
      {Array.from({ length: beatsPerBar - 1 }, (_, i) => {
        const x = ((i + 1) / beatsPerBar) * width;
        return <line key={x} className="grid" x1={x} y1={0} x2={x} y2={height} />;
      })}
      <line className="grid" x1={0} y1={height / 2} x2={width} y2={height / 2} />
      <path className={dense ? "curve thin" : "curve"} d={line(points)} />
    </svg>
  );
}

interface LevelsProps extends Size {
  /** Each level, from 0 to 1, left to right. */
  levels: number[];
}

/** The mixer's levels side by side, for its tab. */
export function LevelsDrawing({ levels, width, height }: LevelsProps) {
  const gap = width / levels.length;
  return (
    <svg className="drawing" aria-hidden="true" width={width} height={height}>
      {levels.map((level, i) => {
        const x = gap * (i + 0.5);
        return (
          <g key={i}>
            <line className="grid" x1={x} y1={1} x2={x} y2={height} />
            <line className="level" x1={x} y1={height} x2={x} y2={height - level * (height - 1)} />
          </g>
        );
      })}
    </svg>
  );
}

/** A choice's small drawing, from its path in a 22 by 10 box (icons.ts). provisional: D-34 */
export function ChoiceIcon({ path }: { path: string }) {
  return (
    <svg className="icon" viewBox="-1 -1 24 12" width={24} height={12} aria-hidden="true">
      <path d={path} />
    </svg>
  );
}
