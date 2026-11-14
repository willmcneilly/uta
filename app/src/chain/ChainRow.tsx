import {
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type {
  EffectKind,
  EffectKindChoice,
  EffectParam,
  EffectSettingView,
  EffectView,
  SoundChoice,
} from "../backend";
import type { FrameLoop } from "../frameLoop";
import { dropIndex } from "../trackOrder";
import { DelayPanel } from "./DelayPanel";
import { MiniTail, type PresetItem, PresetKey } from "./PresetKey";
import { ReverbPanel } from "./ReverbPanel";
import { tailSettings } from "./tailSettings";
import { moveFocus, usePopover } from "./popover";
import "./ChainRow.css";

// The Sound tab as a row: the source's panel, then each effect in the chain,
// then a + to add one. Each panel has a spine down its left side, and a line
// runs along the top of the row from the source through every panel to the
// mixer: the sound's path. provisional: D-30

/** The source's panel, which the master's chain doesn't have. */
export interface Source {
  /** What its spine says: "Synth", "Drums". */
  name: string;
  panel: ReactNode;
  /** The synth's factory sounds, for the preset key on its spine; the kit has none. */
  presets?: { sounds: SoundChoice[]; onChoose: (sound: string) => void };
}

interface Props {
  /** The track's synth or kit, or null for the master's chain, which starts from the tracks. */
  source: Source | null;
  /** The chain, in order. */
  effects: EffectView[];
  /** What the + menu offers. */
  kinds: EffectKindChoice[];
  /** The most effects a chain holds: the + is greyed out at it. */
  maxEffects: number;
  /** The selected effect's ID. */
  selected: string | null;
  onSelect: (effect: string) => void;
  onAdd: (kind: EffectKind) => void;
  onEnabled: (effect: string, enabled: boolean) => void;
  /** Moves an effect to `index` in the chain, counting from 0. */
  onMove: (effect: string, index: number) => void;
  /** Deletes the selected effect: Delete or Backspace on its panel. */
  onRemove: (effect: string) => void;
  /** `gesture` is the same for every change in one drag. */
  onMix: (effect: string, mix: number, gesture?: number) => void;
  onSetting: (effect: string, kind: EffectKind, param: EffectParam, gesture?: number) => void;
  /** Chooses one of an effect's factory sounds, by its key: a Reverb's. */
  onPreset: (effect: string, sound: string) => void;
  /**
   * The effects whose panels are open wide (the Reverb's Early and Late
   * groups): the UI's own, kept by the app, never in the project.
   */
  wide: string[];
  onWide: (effect: string, open: boolean) => void;
  /** One bar at the song's tempo, in seconds, for the Delay's drawing. */
  barSeconds: number;
  beatsPerBar: number;
  /** Where the song is now, in seconds, while it plays; null when it's stopped. */
  songSeconds: ((now: number) => number) | null;
  /** The app's drawing loop, which moves the Delay's light. */
  frames: FrameLoop;
  /** The scrolling row, for the chain map to scroll. */
  rowRef?: RefObject<HTMLDivElement | null>;
}

/**
 * Scrolls `row` just far enough that `panel` is in view: its right edge, or
 * its left edge where it's wider than the row. Smoothly, unless reduced
 * motion is set. provisional: D-41
 */
function revealPanel(row: HTMLElement, panel: HTMLElement): void {
  const box = row.getBoundingClientRect();
  const edges = panel.getBoundingClientRect();
  let by = Math.max(0, edges.right - box.right);
  if (edges.left - by < box.left) by = edges.left - box.left;
  if (by === 0) return;
  const still = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  row.scrollBy?.({ left: by, behavior: still ? "auto" : "smooth" });
}

/** The key folding the source's panel is kept under. */
const SOURCE = "source";

/** An effect being dragged to a new place in the chain. */
interface Reorder {
  from: number;
  to: number;
}

/** The handle: six dots, the mark for "drag me". */
function Grip() {
  const dots = [0, 1, 2].flatMap((row) => [0, 1].map((column) => [column, row]));
  return (
    <svg className="grip" viewBox="0 0 10 14" width={10} height={14} aria-hidden="true">
      {dots.map(([column, row]) => (
        <circle key={`${column}${row}`} cx={2.5 + column * 5} cy={2.5 + row * 4.5} r={1.1} />
      ))}
    </svg>
  );
}

/**
 * The line along the top of a panel's body. Off, it hops over the body: the
 * sound goes round the effect, unchanged.
 */
function Wire({ bypass }: { bypass: boolean }) {
  return (
    <svg className="wire" viewBox="0 0 100 24" preserveAspectRatio="none" aria-hidden="true">
      <path d={bypass ? "M0,12 C3,12 3,3 7,3 L93,3 C97,3 97,12 100,12" : "M0,12 L100,12"} />
    </svg>
  );
}

/** An arrowhead on the line at a panel's right edge: the sound goes on to the next. */
function WireArrow() {
  return (
    <svg className="wire-arrow" viewBox="0 0 8 12" width={8} height={12} aria-hidden="true">
      <path d="M1,2 L7,6 L1,10" />
    </svg>
  );
}

/** The on/off key, sitting on the line like a switch: filled in ink while the effect is on. */
function OnKey({ effect, onEnabled }: { effect: EffectView; onEnabled: (on: boolean) => void }) {
  return (
    <button
      type="button"
      className="on-key"
      aria-pressed={effect.enabled}
      aria-label={`${effect.name} on`}
      title={effect.enabled ? "On. Click to switch off" : "Off. Click to switch on"}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={() => onEnabled(!effect.enabled)}
    >
      <svg viewBox="0 0 12 12" width={12} height={12} aria-hidden="true">
        <path d="M3.4,3.6 A3.8,3.8 0 1 0 8.6,3.6" />
        <line x1="6" y1="1.2" x2="6" y2="5.6" />
      </svg>
    </button>
  );
}

/** How far below the top of the + the menu opens, in CSS pixels: under its key. */
const MENU_DROP = 24;

/**
 * The + on the line at the end of the chain, opening a menu of the kinds of
 * effect. provisional: D-36
 */
function AddEffect({
  kinds,
  full,
  maxEffects,
  onAdd,
}: {
  kinds: EffectKindChoice[];
  full: boolean;
  maxEffects: number;
  onAdd: (kind: EffectKind) => void;
}) {
  const [open, setOpen] = useState(false);
  const { area, key, menu } = usePopover(open, () => setOpen(false), MENU_DROP);
  if (full && open) setOpen(false);

  return (
    <div className="add-effect" ref={area}>
      <button
        ref={key}
        type="button"
        className="add-key"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={full}
        title={full ? `A chain holds at most ${maxEffects} effects` : "Add an effect at the end"}
        onClick={() => setOpen(!open)}
      >
        <span className="plus" aria-hidden="true">
          +
        </span>
        <span className="spine-name">Add effect</span>
      </button>
      {open && (
        <div
          ref={menu}
          className="add-menu"
          role="menu"
          aria-label="Add effect"
          onKeyDown={(event) => moveFocus(area, event)}
        >
          {kinds.map((kind) => (
            <button
              key={kind.kind}
              type="button"
              role="menuitem"
              tabIndex={-1}
              onClick={() => {
                // Focus goes to the new panel once Rust has sent it, not back to the +.
                setOpen(false);
                onAdd(kind.kind);
              }}
            >
              {kind.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Delete or Backspace on its own. With ⌘, ⌃ or ⌥ it's left to the menu bar's
 * shortcuts: ⌘⌫ deletes the track.
 */
function isDeleteKey(event: { key: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean }): boolean {
  if (event.metaKey || event.ctrlKey || event.altKey) return false;
  return event.key === "Delete" || event.key === "Backspace";
}

/** Whether `target` is a control of its own, which takes its own keys and clicks. */
function inControl(target: EventTarget, panel: Element): boolean {
  const control = target instanceof Element ? target.closest("button, input, select, [role=slider]") : null;
  return control !== null && control !== panel && panel.contains(control);
}

/** A share from 0 to 1 as a whole percent: "25%". */
const percent = (share: number) => `${Math.round(share * 100)}%`;

/**
 * A Reverb's factory sounds as its preset menu lists them: by mode, in
 * Mode's order, and in Rust's order within each mode, each
 * with its tail drawn from its settings (described as the Reverb's own are),
 * its name, its suggested Mix and its brief. provisional: D-39
 */
function reverbPresets(effect: EffectView): PresetItem[] {
  const own = (name: string) => effect.settings.find((setting) => setting.name === name);
  const mode = own("mode");
  const items = (effect.sounds ?? []).map((sound) => {
    const value = (name: string) => sound.settings.find((param) => param.name === name)?.value;
    const setting = (name: string): EffectSettingView | undefined => {
      const described = own(name);
      const v = value(name);
      return described && v !== undefined ? ({ ...described, value: v } as EffectSettingView) : undefined;
    };
    const group =
      mode?.kind === "choice" ? mode.options.find((option) => option.value === value("mode"))?.label : undefined;
    return {
      value: sound.value,
      label: sound.label,
      group,
      aside: percent(sound.mix),
      drawing: <MiniTail settings={tailSettings(setting, null)} />,
      about: sound.brief,
    };
  });
  // By mode, in the order Mode lists them; Rust's order within each.
  const order = mode?.kind === "choice" ? mode.options.map((option) => option.label) : [];
  const rank = (item: PresetItem) => (item.group === undefined ? order.length : order.indexOf(item.group));
  return items.map((item, index) => ({ item, index })).sort((a, b) => rank(a.item) - rank(b.item) || a.index - b.index).map(({ item }) => item);
}

export function ChainRow({
  source,
  effects,
  kinds,
  maxEffects,
  selected,
  onSelect,
  onAdd,
  onEnabled,
  onMove,
  onRemove,
  onMix,
  onSetting,
  onPreset,
  barSeconds,
  beatsPerBar,
  songSeconds,
  frames,
  rowRef,
  wide,
  onWide,
}: Props) {
  // Folding a panel to its spine is the UI's own, kept while tracks change,
  // so a folded synth stays folded.
  const [folded, setFolded] = useState<string[]>([]);
  const [reorder, setReorder] = useState<Reorder | null>(null);
  const stopReorder = useRef<(() => void) | null>(null);
  const ownRow = useRef<HTMLDivElement | null>(null);
  // The drop reads the latest chain: an update from Rust can arrive mid-drag.
  const chain = useRef(effects);
  useEffect(() => {
    chain.current = effects;
  }, [effects]);
  // A drag ends if the row goes away mid-drag.
  useEffect(() => () => stopReorder.current?.(), []);

  const fold = (id: string) =>
    setFolded((list) => (list.includes(id) ? list.filter((other) => other !== id) : [...list, id]));

  const foldKey = (id: string, name: string) => (
    <button
      type="button"
      className="fold"
      aria-expanded={!folded.includes(id)}
      title={folded.includes(id) ? `Unfold ${name}` : `Fold ${name}`}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={() => fold(id)}
    >
      <span className="spine-name">{name}</span>
    </button>
  );

  /** Drags an effect by its spine; it lands where the pointer is let go, between the others. */
  const startReorder = (effect: EffectView, from: number, event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    stopReorder.current?.();
    let to = from;
    const middles = () =>
      Array.from(ownRow.current?.querySelectorAll(".wired.effect") ?? []).map((panel) => {
        const box = panel.getBoundingClientRect();
        return (box.left + box.right) / 2;
      });
    const move = (move: globalThis.PointerEvent) => {
      to = dropIndex(middles(), from, move.clientX);
      setReorder({ from, to });
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      window.removeEventListener("keydown", key);
      stopReorder.current = null;
      setReorder(null);
    };
    const end = () => {
      stop();
      // Where the effect is now, in case the chain changed during the drag.
      const now = chain.current.findIndex((other) => other.id === effect.id);
      const index = Math.min(to, chain.current.length - 1);
      if (now >= 0 && index !== now) onMove(effect.id, index);
    };
    const key = (key: globalThis.KeyboardEvent) => {
      if (key.key === "Escape") stop();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    // A drag the window loses ends without moving anything.
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
    window.addEventListener("keydown", key);
    stopReorder.current = stop;
  };

  // Where the dragged panel would land: a line before the panel it would go
  // before, or after the one it would go after.
  const dropMark = (index: number): "before" | "after" | undefined => {
    if (!reorder || reorder.to === reorder.from || index !== reorder.to) return undefined;
    return reorder.to < reorder.from ? "before" : "after";
  };

  // A panel opened wide comes into view once it's drawn wide, so it never
  // opens off the row's edge. provisional: D-41
  const opened = useRef<string | null>(null);
  useLayoutEffect(() => {
    const row = ownRow.current;
    const effect = opened.current;
    if (effect === null || !row) return;
    const panel = row.querySelector<HTMLElement>(`[data-panel="${effect}"]`);
    // Wait for the render that draws it wide.
    if (!panel?.querySelector("[data-open]")) return;
    opened.current = null;
    revealPanel(row, panel);
  });
  const openWide = (effect: string, open: boolean) => {
    opened.current = open ? effect : null;
    onWide(effect, open);
  };

  // A panel selected from outside the row (a new effect, once Rust has sent
  // it) takes focus, unless something in the row has it, so Delete deletes it.
  useEffect(() => {
    const row = ownRow.current;
    if (!row || selected === null || row.contains(document.activeElement)) return;
    row.querySelector<HTMLElement>(`[data-panel="${selected}"]`)?.focus();
  }, [selected]);

  // With nothing focused (after a click on something that doesn't take
  // focus), Delete still deletes the panel shown as selected.
  const latestSelected = useRef(selected);
  const latestRemove = useRef(onRemove);
  useEffect(() => {
    latestSelected.current = selected;
    latestRemove.current = onRemove;
  });
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.target !== document.body || latestSelected.current === null) return;
      if (!isDeleteKey(event)) return;
      event.preventDefault();
      latestRemove.current(latestSelected.current);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const removeKey = (effect: EffectView) => (event: KeyboardEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget) return;
    if (!isDeleteKey(event)) return;
    event.preventDefault();
    onRemove(effect.id);
  };

  return (
    <div
      className="chain-row"
      data-reordering={reorder !== null || undefined}
      ref={(element) => {
        ownRow.current = element;
        if (rowRef) rowRef.current = element;
      }}
    >
      {source ? (
        // Unnamed: the synth or kit panel inside names itself.
        <section className="wired source" data-panel={SOURCE}>
          <header className="spine">
            <span className="wire-start" aria-hidden="true" />
            {foldKey(SOURCE, source.name)}
            {source.presets && (
              <PresetKey
                label={`${source.name} sounds`}
                items={source.presets.sounds}
                onChoose={source.presets.onChoose}
              />
            )}
          </header>
          {!folded.includes(SOURCE) && (
            <div className="wired-body">
              <Wire bypass={false} />
              {source.panel}
            </div>
          )}
          <WireArrow />
        </section>
      ) : (
        <section className="wired source tracks-in">
          <header className="spine">
            <span className="wire-start" aria-hidden="true" />
            <span className="spine-name quiet">Tracks</span>
          </header>
          <WireArrow />
        </section>
      )}

      {effects.map((effect, index) => (
        <section
          key={effect.id}
          className="wired effect"
          aria-label={effect.name}
          aria-current={selected === effect.id ? "true" : undefined}
          tabIndex={0}
          data-panel={effect.id}
          data-off={!effect.enabled || undefined}
          data-dragging={reorder?.from === index || undefined}
          data-drop={dropMark(index)}
          onPointerDown={(event) => {
            onSelect(effect.id);
            // A click on the panel itself, rather than one of its controls,
            // focuses it, so Delete deletes it.
            if (!inControl(event.target, event.currentTarget)) event.currentTarget.focus();
          }}
          onFocus={() => onSelect(effect.id)}
          onKeyDown={removeKey(effect)}
        >
          <header
            className="spine"
            title="Drag to move"
            onPointerDown={(event) => {
              onSelect(effect.id);
              event.currentTarget.parentElement?.focus();
              startReorder(effect, index, event);
            }}
          >
            <OnKey effect={effect} onEnabled={(on) => onEnabled(effect.id, on)} />
            {foldKey(effect.id, effect.name)}
            {effect.sounds && (
              <PresetKey
                label={`${effect.name} sounds`}
                items={reverbPresets(effect)}
                heading={{ name: "Factory sounds", aside: "Suggested Mix" }}
                foot={`Mix stays at ${percent(effect.mix.value)}: it's yours, not the sound's.`}
                onChoose={(sound) => onPreset(effect.id, sound)}
              />
            )}
            <span className="handle" aria-hidden="true">
              <Grip />
            </span>
          </header>
          {!folded.includes(effect.id) && (
            <div className="wired-body">
              <Wire bypass={!effect.enabled} />
              {effect.kind === "reverb" ? (
                <ReverbPanel
                  effect={effect}
                  open={wide.includes(effect.id)}
                  onOpen={(open) => openWide(effect.id, open)}
                  barSeconds={barSeconds}
                  songSeconds={songSeconds}
                  frames={frames}
                  onSetting={(param, gesture) => onSetting(effect.id, effect.kind, param, gesture)}
                  onMix={(mix, gesture) => onMix(effect.id, mix, gesture)}
                />
              ) : (
                <DelayPanel
                  effect={effect}
                  barSeconds={barSeconds}
                  beatsPerBar={beatsPerBar}
                  songSeconds={songSeconds}
                  frames={frames}
                  onSetting={(param, gesture) => onSetting(effect.id, effect.kind, param, gesture)}
                  onMix={(mix, gesture) => onMix(effect.id, mix, gesture)}
                />
              )}
            </div>
          )}
          <WireArrow />
        </section>
      ))}

      <div className="wired-end">
        <AddEffect
          kinds={kinds}
          full={effects.length >= maxEffects}
          maxEffects={maxEffects}
          onAdd={onAdd}
        />
        <WireArrow />
        <div className="mixer-stub">
          <span className="spine-name quiet">{source ? "Mixer" : "Out"}</span>
        </div>
      </div>
    </div>
  );
}

/**
 * A map of the chain, in the tab bar: each panel's name, in order, as a flat
 * key that scrolls the row to it. An effect that's off is in the third ink.
 * provisional: D-30
 */
export function ChainMap({
  source,
  effects,
  rowRef,
}: {
  source: string | null;
  effects: EffectView[];
  rowRef: RefObject<HTMLDivElement | null>;
}) {
  const go = (panel: string) =>
    rowRef.current
      ?.querySelector<HTMLElement>(`[data-panel="${panel}"]`)
      ?.scrollIntoView?.({ behavior: "smooth", inline: "nearest", block: "nearest" });
  const items = [
    ...(source === null ? [] : [{ id: SOURCE, name: source, on: true }]),
    ...effects.map((effect) => ({ id: effect.id, name: effect.name, on: effect.enabled })),
  ];
  return (
    <nav className="chain-map" aria-label="Chain">
      {items.map((item, index) => (
        <span key={item.id} className="chain-step">
          {index > 0 && (
            <span className="chain-arrow" aria-hidden="true">
              ›
            </span>
          )}
          <button type="button" data-off={!item.on || undefined} onClick={() => go(item.id)}>
            {item.name}
          </button>
        </span>
      ))}
    </nav>
  );
}
