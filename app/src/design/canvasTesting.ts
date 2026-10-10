// A 2D context for tests that records what it's asked to draw, with the
// fill and stroke it was drawn in.

export interface Mark {
  call: string;
  style: string;
  args: unknown[];
}

export function recordingContext(marks: Mark[]): CanvasRenderingContext2D {
  const state: Record<string, unknown> = { fillStyle: "", strokeStyle: "", lineWidth: 1, font: "" };
  const style = (call: string) =>
    call.startsWith("stroke") ? String(state.strokeStyle) : String(state.fillStyle);
  return new Proxy(state, {
    get: (target, key) =>
      typeof key === "string" && key in target
        ? target[key]
        : (...args: unknown[]) => marks.push({ call: String(key), style: style(String(key)), args }),
    set: (target, key, value) => {
      target[key as string] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}
