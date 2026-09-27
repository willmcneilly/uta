import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// Vitest's globals are off, so Testing Library can't register this itself.
afterEach(cleanup);

// jsdom has no canvas. The meter skips drawing without a 2D context, so the
// tests don't need one; this just stops jsdom logging "not implemented".
vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
