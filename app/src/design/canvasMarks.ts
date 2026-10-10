// Marks drawn the same way on both canvases, the timeline and the piano
// roll: the playhead, and the selection box.

/** The playhead's marker in the ruler: a triangle this wide and tall, pointing down. */
const MARKER_HALF_WIDTH = 5.5;
const MARKER_HEIGHT = 7;

/** A box on the canvas, in CSS pixels. */
export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The playhead: a line `width` wide from the canvas's top to `height` at
 * `x`, in the live ink, with a marker in the ruler centred on the line.
 */
export function drawPlayhead(
  context: CanvasRenderingContext2D,
  x: number,
  width: number,
  height: number,
  colour: string,
): void {
  context.fillStyle = colour;
  context.fillRect(x, 0, width, height);
  const middle = x + width / 2;
  context.beginPath();
  context.moveTo(middle - MARKER_HALF_WIDTH, 0);
  context.lineTo(middle + MARKER_HALF_WIDTH, 0);
  context.lineTo(middle, MARKER_HEIGHT);
  context.closePath();
  context.fill();
}

/** The selection box: a wash, edged inside its box with a line `line` wide. */
export function drawSelectionBox(
  context: CanvasRenderingContext2D,
  box: Box,
  wash: string,
  edge: string,
  line: number,
): void {
  context.fillStyle = wash;
  context.fillRect(box.x, box.y, box.width, box.height);
  context.strokeStyle = edge;
  context.lineWidth = line;
  context.strokeRect(box.x + line / 2, box.y + line / 2, box.width - line, box.height - line);
}
