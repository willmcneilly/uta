/// <reference lib="dom" />
// The listening page's own code, run in the browser. listen.ts serves it with
// its types stripped, so it imports nothing and uses only erasable syntax.
//
// Every candidate plays at once, from one start time, looping the same
// stretch, each through its own gain: switching only moves the gains, so it's
// instant and lands on the same beat. The audio arrives already matched in
// loudness, and labelled by letter, so nothing here knows which is which
// until Finish.

interface State {
  index: number;
  total: number;
  question: string;
  kind: "pick one" | "rate each" | "listen and describe";
  labels: string[];
  loop: { start: number; end: number; bpm: number | null };
  loudnessMatched: boolean;
  ratingRange: [number, number];
}

interface Reveal {
  verdict: string;
  prediction: { text: string; choice?: string; ratings?: Record<string, number>; measurements: string[] };
  candidates: { label: string; id: string; gainDb: number; measurements: Record<string, number | null> }[];
  next: boolean;
}

/** How long a switch takes to fade across, in seconds: short enough to feel instant, long enough not to click. */
const SWITCH_SECONDS = 0.005;

const app = document.getElementById("app") as HTMLElement;

/** An element with these properties (className, type, …) and children. */
function element<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...children: (Node | string)[]): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  Object.assign(node, props);
  node.append(...children);
  return node;
}

async function start(): Promise<void> {
  const response = await fetch("/api/session");
  if (!response.ok) {
    app.replaceChildren(element("p", { className: "hint" }, "That was the last question. You can close this tab."));
    return;
  }
  const state = (await response.json()) as State;
  const context = new AudioContext();
  const buffers = await Promise.all(
    state.labels.map(async (label) => context.decodeAudioData(await (await fetch(`/audio/${label}`)).arrayBuffer())),
  );
  ask(state, context, buffers);
}

function ask(state: State, context: AudioContext, buffers: AudioBuffer[]): void {
  const { labels, loop } = state;
  const length = loop.end - loop.start;
  const gains = labels.map(() => {
    const gain = context.createGain();
    gain.gain.value = 0;
    gain.connect(context.destination);
    return gain;
  });
  let hearing = 0;
  let sources: AudioBufferSourceNode[] = [];
  let startedAt = 0;
  let frame = 0;

  const position = element("span", { className: "position" }, "–");
  const playKey = element("button", { className: "key play primary", type: "button" });
  const setPlayKey = (playing: boolean) => {
    playKey.replaceChildren(element("span", { className: "mark" }, playing ? "■" : "▶"), playing ? "Stop" : "Play");
    app.classList.toggle("playing", playing);
  };
  setPlayKey(false);

  const showPosition = () => {
    // Before the first beat (it starts a moment after Play), count from the top.
    const at = Math.max(0, context.currentTime - startedAt) % length;
    if (loop.bpm) {
      const beat = Math.floor((at * loop.bpm) / 60);
      position.textContent = `${Math.floor(beat / 4) + 1}.${(beat % 4) + 1}`;
    } else position.textContent = `${at.toFixed(1)} s`;
    frame = requestAnimationFrame(showPosition);
  };

  const play = () => {
    void context.resume();
    startedAt = context.currentTime + 0.05;
    sources = buffers.map((buffer, i) => {
      const source = context.createBufferSource();
      source.buffer = buffer;
      source.loop = true;
      source.loopStart = loop.start;
      source.loopEnd = loop.end;
      source.connect(gains[i]);
      source.start(startedAt, loop.start);
      return source;
    });
    gains.forEach((gain, i) => gain.gain.setValueAtTime(i === hearing ? 1 : 0, startedAt));
    setPlayKey(true);
    showPosition();
  };
  const stop = () => {
    sources.forEach((source) => source.stop());
    sources = [];
    cancelAnimationFrame(frame);
    position.textContent = "–";
    setPlayKey(false);
  };
  playKey.addEventListener("click", () => (sources.length ? stop() : play()));

  const hear = (i: number) => {
    hearing = i;
    const now = context.currentTime;
    gains.forEach((gain, j) => {
      gain.gain.cancelScheduledValues(now);
      gain.gain.setValueAtTime(gain.gain.value, now);
      gain.gain.linearRampToValueAtTime(j === i ? 1 : 0, now + SWITCH_SECONDS);
    });
    hearButtons.forEach((button, j) => button.setAttribute("aria-checked", String(j === i)));
  };

  const segments = (name: string, className: string, onChoose: (i: number) => void) => {
    const group = element("div", { className: `segments ${className}` });
    group.setAttribute("role", "radiogroup");
    group.setAttribute("aria-label", name);
    const buttons = labels.map((label, i) => {
      const button = element("button", { type: "button" }, label);
      button.setAttribute("role", "radio");
      button.setAttribute("aria-checked", "false");
      button.addEventListener("click", () => onChoose(i));
      return button;
    });
    group.append(...buttons);
    return { group, buttons };
  };

  const hearRow = segments("Listen to", "hear", hear);
  const hearButtons = hearRow.buttons;
  hear(0);

  let choice: number | null = null;
  const ratings: Record<string, number> = {};
  const note = element("textarea", {
    placeholder: state.kind === "listen and describe" ? "What do you hear? What's wrong with it?" : "Why? In your own words (optional)",
  });
  const error = element("p", { className: "error" });
  error.hidden = true;
  const finishKey = element("button", { className: "key primary", type: "button" }, "Finish");

  const ready = () =>
    state.kind === "pick one"
      ? choice !== null
      : state.kind === "rate each"
        ? labels.every((label) => label in ratings)
        : note.value.trim() !== "";
  const update = () => (finishKey.disabled = !ready());
  note.addEventListener("input", update);

  const answer = element("section");
  if (state.kind === "pick one") {
    const pick = segments("Your pick", "pick", (i) => {
      choice = i;
      pick.buttons.forEach((button, j) => button.setAttribute("aria-checked", String(j === i)));
      update();
    });
    answer.append(element("h2", {}, "Your pick"), pick.group);
  } else if (state.kind === "rate each") {
    const [low, high] = state.ratingRange;
    const grid = element("div", { className: "rate" });
    for (const label of labels) {
      const row = element("div", { className: "segments pick" });
      row.setAttribute("role", "radiogroup");
      row.setAttribute("aria-label", `Rate ${label}`);
      const buttons: HTMLButtonElement[] = [];
      for (let n = low; n <= high; n++) {
        const button = element("button", { type: "button" }, String(n));
        button.setAttribute("role", "radio");
        button.setAttribute("aria-checked", "false");
        button.addEventListener("click", () => {
          ratings[label] = n;
          buttons.forEach((b, j) => b.setAttribute("aria-checked", String(j + low === n)));
          update();
        });
        buttons.push(button);
      }
      row.append(...buttons);
      grid.append(element("span", {}, label), row);
    }
    answer.append(element("h2", {}, `Rate each, ${low} (worst) to ${high} (best)`), grid);
  }

  finishKey.addEventListener("click", async () => {
    if (!ready()) return;
    finishKey.disabled = true;
    const body: { note: string; choice?: string; ratings?: Record<string, number> } = { note: note.value };
    if (choice !== null) body.choice = labels[choice];
    if (state.kind === "rate each") body.ratings = ratings;
    const response = await fetch("/api/finish", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
    const result = (await response.json()) as Reveal | { error: string };
    if ("error" in result) {
      error.textContent = result.error;
      error.hidden = false;
      update();
      return;
    }
    // The listening row stays, so the candidates can be heard again knowing which is which.
    showReveal(state, result, listen, () => {
      stop();
      void context.close();
      app.replaceChildren(element("p", { className: "hint" }, "Loading…"));
      void start();
    });
  });
  update();

  // Keys: space plays and stops, a letter (or its number) switches to it.
  document.onkeydown = (event) => {
    if (event.target instanceof HTMLTextAreaElement || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === " ") {
      event.preventDefault();
      playKey.click();
      return;
    }
    const byLetter = labels.indexOf(event.key.toUpperCase());
    const byNumber = Number(event.key) - 1;
    const i = byLetter >= 0 ? byLetter : Number.isInteger(byNumber) && byNumber >= 0 && byNumber < labels.length ? byNumber : -1;
    if (i >= 0) hear(i);
  };

  const fair = [
    state.loudnessMatched ? "Matched in loudness." : "Not matched in loudness: the levels are as rendered.",
    "Labels are shuffled.",
    labels.length > 1 ? `Press ${labels.join(", ")} (or 1 to ${labels.length}) to switch, space to play.` : "Space plays and stops.",
  ].join(" ");

  const listen = element(
    "section",
    {},
    element("h2", {}, labels.length > 1 ? "Listen to" : "Listen"),
    element("div", { className: "row" }, playKey, position, ...(labels.length > 1 ? [hearRow.group] : [])),
    element("p", { className: "hint" }, fair),
  );
  app.replaceChildren(
    element(
      "header",
      {},
      element("div", { className: "progress" }, `Question ${state.index + 1} of ${state.total}`),
      element("h1", {}, state.question),
    ),
    listen,
    answer,
    element("section", {}, element("h2", {}, "Note"), note),
    element("div", { className: "row" }, finishKey, error),
  );
}

function showReveal(state: State, reveal: Reveal, listen: HTMLElement, next: () => void): void {
  const { prediction, candidates } = reveal;
  const predicted = prediction.choice ? candidates.find((c) => c.id === prediction.choice) : undefined;
  const keys = [
    ...prediction.measurements,
    ...Object.keys(candidates[0]?.measurements ?? {}).filter((key) => !prediction.measurements.includes(key)),
  ];
  const format = (value: number | null | undefined) => (value === null || value === undefined ? "–" : Math.abs(value) >= 100 ? value.toFixed(0) : value.toPrecision(3));
  const table = element(
    "table",
    {},
    element(
      "thead",
      {},
      element("tr", {}, element("th", {}, ""), ...candidates.map((c) => element("th", {}, `${c.label}: ${c.id}`))),
    ),
    element(
      "tbody",
      {},
      element("tr", {}, element("td", {}, "gain applied, dB"), ...candidates.map((c) => element("td", {}, format(c.gainDb)))),
      ...keys.map((key) => {
        const row = element("tr", {}, element("td", {}, key), ...candidates.map((c) => element("td", {}, format(c.measurements[key]))));
        if (prediction.measurements.includes(key)) row.className = "relied";
        return row;
      }),
    ),
  );
  const nextKey = element("button", { className: "key primary", type: "button" }, "Next question");
  nextKey.addEventListener("click", next);
  app.replaceChildren(
    element(
      "header",
      {},
      element("div", { className: "progress" }, `Question ${state.index + 1} of ${state.total} · saved to ${reveal.verdict}`),
      element("h1", {}, state.question),
    ),
    listen,
    element(
      "section",
      {},
      element("h2", {}, "Which was which"),
      element("p", {}, candidates.map((c) => `${c.label} was ${c.id}`).join(", ") + "."),
    ),
    element(
      "section",
      { className: "prediction" },
      element("h2", {}, predicted ? `Claude predicted ${predicted.label} (${predicted.id})` : "Claude predicted"),
      element("p", {}, prediction.text),
    ),
    element("section", {}, element("h2", {}, "The numbers (the ones the prediction relies on first)"), table),
    reveal.next ? nextKey : element("p", { className: "hint" }, "That was the last question. You can close this tab."),
  );
}

void start();
