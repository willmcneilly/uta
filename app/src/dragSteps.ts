// One step in flight at a time, latest wins (RFC-004, part 3). A drag makes
// a step every time the pointer moves, faster than a big song can take them.
// Sending each as it comes would build a queue in Rust that's still being
// worked through seconds after the mouse stops. Instead, while a step is on
// its way, newer steps of the same drag replace each other, and only the
// newest goes when the reply comes back. This is called coalescing.

/** Sends one command and handles its reply, or reports its failure. */
export type Send = () => Promise<unknown>;

interface Waiting {
  send: Send;
  /** The drag a step belongs to; `undefined` for anything that's never replaced. */
  gesture?: number;
}

/**
 * Sends a drag's steps one at a time, latest wins. Everything sent through
 * it goes in the order it came, one command at a time, except that a step
 * replaces a waiting step of the same drag. So what a drag sends when it
 * ends (a trim, a cancel) always follows its final step.
 */
export class DragSteps {
  private inFlight = false;
  private waiting: Waiting[] = [];

  /**
   * A step of drag `gesture`: replaces the step of the same drag that's
   * waiting to go, if that's the last thing waiting. A change with no
   * gesture (a slider moved with the keyboard, say) is never replaced.
   */
  step(gesture: number | undefined, send: Send): void {
    const last = this.waiting.at(-1);
    if (gesture !== undefined && last?.gesture === gesture) {
      this.waiting[this.waiting.length - 1] = { send, gesture };
    } else {
      this.waiting.push({ send, gesture });
    }
    this.next();
  }

  /** Sends `send` after everything before it, and never replaces it: a drag's trim, say. */
  then(send: Send): void {
    this.waiting.push({ send });
    this.next();
  }

  /**
   * Esc mid-drag: drops `gesture`'s waiting steps, and sends `send` (the
   * cancel) after the step in flight, so it puts back everything Rust got.
   */
  cancel(gesture: number, send: Send): void {
    this.waiting = this.waiting.filter((waiting) => waiting.gesture !== gesture);
    this.then(send);
  }

  private next(): void {
    if (this.inFlight) return;
    const waiting = this.waiting.shift();
    if (!waiting) return;
    this.inFlight = true;
    const done = () => {
      this.inFlight = false;
      this.next();
    };
    // A failed step is its sender's to report; the drag carries on.
    let sent: Promise<unknown>;
    try {
      sent = waiting.send();
    } catch {
      sent = Promise.resolve();
    }
    sent.then(done, done);
  }
}
