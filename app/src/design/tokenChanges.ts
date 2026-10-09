import { useSyncExternalStore } from "react";
import { useColourScheme } from "../useColourScheme";

// The canvases and the meter read the tokens when they're built. Release
// builds rebuild them only when the colour scheme changes. In development
// builds the tuning panel also announces each change it makes, so they read
// the tokens again while you tune.

let version = 0;
const listeners = new Set<() => void>();

/** Called by the tuning panel after it changes a token or the theme. */
export function tokensChanged(): void {
  version += 1;
  for (const listener of listeners) listener();
}

// In a release build nothing announces changes, so nothing listens.
const subscribe = import.meta.env.DEV
  ? (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    }
  : () => () => {};

/**
 * Changes whenever whatever draws from the tokens should read them again: a
 * new colour scheme, or in development builds, a change from the tuning panel.
 */
export function useTokenVersion(): string {
  const scheme = useColourScheme();
  const changes = useSyncExternalStore(subscribe, () => version);
  return `${scheme} ${changes}`;
}
