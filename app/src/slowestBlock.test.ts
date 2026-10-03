import { describe, expect, it } from "vitest";
import { percent } from "./slowestBlock";

describe("percent", () => {
  it("writes a share of the deadline as a percentage", () => {
    expect(percent(0)).toBe("0.0%");
    expect(percent(0.004)).toBe("0.4%");
    expect(percent(0.0999)).toBe("10.0%");
    expect(percent(0.25)).toBe("25%");
    expect(percent(1.7)).toBe("170%");
  });
});
