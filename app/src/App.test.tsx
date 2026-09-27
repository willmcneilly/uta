import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("renders an empty window", () => {
    const { container } = render(<App />);
    expect(container.querySelector("main.app")).toBeEmptyDOMElement();
  });
});
