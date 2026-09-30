import { useEffect, useState } from "react";

/** "light" or "dark", following the system, so the canvases redraw in the new colours. */
export function useColourScheme(): "light" | "dark" {
  const [query] = useState(() =>
    typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-color-scheme: dark)")
      : null,
  );
  const [dark, setDark] = useState(() => query?.matches ?? false);
  useEffect(() => {
    if (!query) return;
    const onChange = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [query]);
  return dark ? "dark" : "light";
}
