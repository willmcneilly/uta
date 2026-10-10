// The bar ruler across the top of the timeline and the piano roll, drawn on
// a canvas: bar numbers, ticks standing on its foot, and the loop region as
// a dimension line (provisional: D-2).

/** Bar numbers are at least this far apart. */
const MIN_BAR_LABEL_SPACING = 32;
/** The bar ticks and beat ticks, up from the ruler's foot. */
const BAR_TICK_HEIGHT = 8;
const BEAT_TICK_HEIGHT = 4;
/** Beat ticks are drawn only when they're at least this far apart. */
const MIN_BEAT_TICK_SPACING = 6;
/** Where a bar number sits, from its bar line and the ruler's top. */
const LABEL_INSET = 4;
/** The loop's dimension line, this far above the ruler's foot. */
const LOOP_LINE_RAISE = 5;
/** Its end ticks run from here to the ruler's foot. */
const LOOP_TICK_HEIGHT = 10;
/** Its arrowheads, and the narrowest loop that has room for them. */
const LOOP_ARROW_LENGTH = 5;
const LOOP_ARROW_HALF_WIDTH = 2.5;
const LOOP_ARROWS_MIN_WIDTH = 4 * LOOP_ARROW_LENGTH;

/** The ruler's colours, from the canvas's theme. */
export interface RulerColours {
  /** The sheet under it. */
  background: string;
  /** The wash over the loop region while the loop is on. */
  loopRegion: string;
  /** The line along its foot. */
  edge: string;
  beatTick: string;
  barTick: string;
  /** The bar numbers. */
  text: string;
  /** The loop's dimension line while the loop is on, and while it's off. */
  loop: string;
  loopOff: string;
}

/** What a ruler draws, and where. */
export interface Ruler {
  /** Its left edge and width on the canvas. It runs from the canvas's top. */
  left: number;
  width: number;
  height: number;
  /** Where a tick is drawn across the canvas. */
  tickToX: (tick: number) => number;
  pixelsPerTick: number;
  /** The ticks in view. */
  ticks: { start: number; end: number };
  ticksPerQuarter: number;
  /** A bar's length, in ticks. */
  bar: number;
  /** The loop region, in ticks. */
  loop: { start: number; end: number; enabled: boolean };
  /** The weight of its ticks and foot (`stroke-grid`), and of the loop's line (`stroke-clip`). */
  line: number;
  loopWeight: number;
  /** The bar numbers' canvas font. */
  font: string;
  colours: RulerColours;
}

/** Draws the ruler, clipped to its box. */
export function drawRuler(context: CanvasRenderingContext2D, ruler: Ruler): void {
  const { left, width, height, tickToX, ticks, ticksPerQuarter, bar, loop, line, colours } = ruler;
  const right = left + width;
  context.save();
  context.beginPath();
  context.rect(left, 0, width, height);
  context.clip();
  context.fillStyle = colours.background;
  context.fillRect(left, 0, width, height);
  const loopLeft = Math.max(left, Math.round(tickToX(loop.start)));
  const loopRight = Math.min(right, Math.round(tickToX(loop.end)));
  if (loop.enabled && loopRight > loopLeft) {
    context.fillStyle = colours.loopRegion;
    context.fillRect(loopLeft, 0, loopRight - loopLeft, height);
  }
  context.fillStyle = colours.edge;
  context.fillRect(left, height - line, width, line);

  // Beat ticks when they're far enough apart, and bar ticks with their
  // numbers, all standing on the ruler's bottom edge.
  const bottom = height - line;
  if (ticksPerQuarter * ruler.pixelsPerTick >= MIN_BEAT_TICK_SPACING) {
    context.fillStyle = colours.beatTick;
    for (
      let tick = Math.floor(ticks.start / ticksPerQuarter) * ticksPerQuarter;
      tick < ticks.end;
      tick += ticksPerQuarter
    ) {
      if (tick % bar !== 0) {
        context.fillRect(Math.round(tickToX(tick)), bottom - BEAT_TICK_HEIGHT, line, BEAT_TICK_HEIGHT);
      }
    }
  }
  context.font = ruler.font;
  context.textBaseline = "top";
  const labelEvery = Math.max(1, Math.ceil(MIN_BAR_LABEL_SPACING / (bar * ruler.pixelsPerTick)));
  const barStep = bar * labelEvery;
  for (let tick = Math.floor(ticks.start / barStep) * barStep; tick < ticks.end; tick += barStep) {
    const x = Math.round(tickToX(tick));
    context.fillStyle = colours.barTick;
    context.fillRect(x, bottom - BAR_TICK_HEIGHT, line, BAR_TICK_HEIGHT);
    context.fillStyle = colours.text;
    context.fillText(String(tick / bar + 1), x + LABEL_INSET, LABEL_INSET);
  }
  drawLoop(context, ruler);
  context.restore();
}

/**
 * The loop region, drawn like a dimension line on a drawing: a line from
 * its start to its end with an arrowhead and a tick at each, in ink while
 * the loop is on and faint while it's off.
 */
function drawLoop(context: CanvasRenderingContext2D, ruler: Ruler): void {
  const { left, width, height, tickToX, loop, colours } = ruler;
  const start = Math.round(tickToX(loop.start));
  const end = Math.round(tickToX(loop.end));
  const from = Math.max(left, start);
  const to = Math.min(left + width, end);
  if (to <= from) return;
  const weight = ruler.loopWeight;
  const y = height - LOOP_LINE_RAISE;
  const middle = y + weight / 2;
  context.fillStyle = loop.enabled ? colours.loop : colours.loopOff;
  context.fillRect(from, y, to - from, weight);
  // The end ticks stand on the bar lines the loop starts and ends on, inside it.
  context.fillRect(start, height - LOOP_TICK_HEIGHT, weight, LOOP_TICK_HEIGHT);
  context.fillRect(end - weight, height - LOOP_TICK_HEIGHT, weight, LOOP_TICK_HEIGHT);
  if (end - start < LOOP_ARROWS_MIN_WIDTH) return;
  context.beginPath();
  context.moveTo(start + weight, middle);
  context.lineTo(start + weight + LOOP_ARROW_LENGTH, middle - LOOP_ARROW_HALF_WIDTH);
  context.lineTo(start + weight + LOOP_ARROW_LENGTH, middle + LOOP_ARROW_HALF_WIDTH);
  context.closePath();
  context.moveTo(end - weight, middle);
  context.lineTo(end - weight - LOOP_ARROW_LENGTH, middle - LOOP_ARROW_HALF_WIDTH);
  context.lineTo(end - weight - LOOP_ARROW_LENGTH, middle + LOOP_ARROW_HALF_WIDTH);
  context.closePath();
  context.fill();
}
