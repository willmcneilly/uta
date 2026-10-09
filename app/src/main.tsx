import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// The tuning panel, in development builds only. In a release build this
// condition is false when Vite builds it, so the panel isn't in the bundle.
if (import.meta.env.DEV) {
  void import("./design/tuning/panel").then(({ openTuningPanel }) => openTuningPanel());
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
