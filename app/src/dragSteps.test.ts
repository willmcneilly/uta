import { describe, expect, it } from "vitest";
import { DragSteps } from "./dragSteps";

/** A stand-in for Rust: records each command sent, and replies when told. */
class Rust {
  sent: string[] = [];
  private replies: { resolve: () => void; reject: (reason: unknown) => void }[] = [];

  /** A command that's sent as `name`, and answered by `reply` or `fail`. */
  command = (name: string) => () =>
    new Promise<void>((resolve, reject) => {
      this.sent.push(name);
      this.replies.push({ resolve, reject });
    });

  get inFlight(): number {
    return this.replies.length;
  }

  async reply(): Promise<void> {
    this.replies.shift()!.resolve();
    await settle();
  }

  async fail(): Promise<void> {
    this.replies.shift()!.reject(new Error("no such track"));
    await settle();
  }
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("DragSteps", () => {
  it("sends the first step at once, and holds the next until it's answered", async () => {
    const rust = new Rust();
    const drags = new DragSteps();
    drags.step(1, rust.command("a"));
    drags.step(1, rust.command("b"));
    expect(rust.sent).toEqual(["a"]);
    await rust.reply();
    expect(rust.sent).toEqual(["a", "b"]);
  });

  it("keeps only the newest of the steps that wait, and always sends the final one", async () => {
    const rust = new Rust();
    const drags = new DragSteps();
    for (const name of ["a", "b", "c", "d"]) drags.step(1, rust.command(name));
    expect(rust.inFlight).toBe(1);
    await rust.reply();
    expect(rust.inFlight).toBe(1);
    await rust.reply();
    expect(rust.sent).toEqual(["a", "d"]);
    expect(rust.inFlight).toBe(0);
  });

  it("never replaces a step of another drag, or a change with no gesture", async () => {
    const rust = new Rust();
    const drags = new DragSteps();
    drags.step(1, rust.command("a"));
    drags.step(1, rust.command("b"));
    drags.step(2, rust.command("c"));
    drags.step(undefined, rust.command("key 1"));
    drags.step(undefined, rust.command("key 2"));
    for (let i = 0; i < 5; i++) if (rust.inFlight) await rust.reply();
    expect(rust.sent).toEqual(["a", "b", "c", "key 1", "key 2"]);
  });

  it("sends what a drag sends when it ends after its final step, and never replaces it", async () => {
    const rust = new Rust();
    const drags = new DragSteps();
    drags.step(1, rust.command("a"));
    drags.step(1, rust.command("b"));
    drags.step(1, rust.command("c"));
    drags.then(rust.command("trim"));
    for (let i = 0; i < 3; i++) await rust.reply();
    expect(rust.sent).toEqual(["a", "c", "trim"]);
  });

  it("on cancel, drops the drag's waiting steps and cancels after the step in flight", async () => {
    const rust = new Rust();
    const drags = new DragSteps();
    drags.then(rust.command("add"));
    drags.step(1, rust.command("a"));
    drags.step(1, rust.command("b"));
    drags.cancel(1, rust.command("cancel"));
    expect(rust.sent).toEqual(["add"]);
    for (let i = 0; i < 3; i++) if (rust.inFlight) await rust.reply();
    // The add waited ahead of the steps, so it isn't dropped.
    expect(rust.sent).toEqual(["add", "cancel"]);
  });

  it("carries on after a step fails", async () => {
    const rust = new Rust();
    const drags = new DragSteps();
    drags.step(1, rust.command("a"));
    drags.step(1, rust.command("b"));
    await rust.fail();
    expect(rust.sent).toEqual(["a", "b"]);
    await rust.reply();
    drags.step(1, rust.command("c"));
    expect(rust.sent).toEqual(["a", "b", "c"]);
  });

  it("carries on after a step throws before sending", async () => {
    const rust = new Rust();
    const drags = new DragSteps();
    drags.step(1, () => {
      throw new Error("bad step");
    });
    drags.step(1, rust.command("b"));
    await settle();
    expect(rust.sent).toEqual(["b"]);
  });
});
