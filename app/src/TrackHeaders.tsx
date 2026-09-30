import { type PointerEvent, useEffect, useRef, useState } from "react";
import type { MixerLimits, MixerView, ProjectView, TrackView } from "./backend";
import { Meter } from "./Meter";
import type { TrackLevels } from "./meterLevel";
import { dropIndex, formatPan } from "./trackOrder";
import { useGesture } from "./useGesture";

/** How wide each header's meter is, in CSS pixels. */
const METER_WIDTH = 180;

interface Props {
  project: ProjectView;
  /** The selected track's ID. */
  selected: string | null;
  levels: TrackLevels;
  onSelect: (track: string) => void;
  /** `gesture` is the same for every change in one drag. */
  onMixer: (track: string, mixer: MixerView, gesture?: number) => void;
  /** Solos a track on its own: ⌥-click on Solo. */
  onSoloAlone: (track: string) => void;
  onAdd: () => void;
  /** Moves a track to `index` in the order, counting from 0 at the top. */
  onMove: (track: string, index: number) => void;
}

/** A header being dragged to a new place in the order. */
interface Reorder {
  from: number;
  to: number;
}

function formatDb(db: number): string {
  return `${db.toFixed(1)} dB`;
}

/**
 * Every track's header, top to bottom, with **+ Add track** below. Each has
 * the track's name, mute, solo, volume, pan and a meter. Drag a header by
 * its name to reorder the tracks.
 */
export function TrackHeaders({
  project,
  selected,
  levels,
  onSelect,
  onMixer,
  onSoloAlone,
  onAdd,
  onMove,
}: Props) {
  const listRef = useRef<HTMLOListElement>(null);
  const [reorder, setReorder] = useState<Reorder | null>(null);
  const stopReorder = useRef<(() => void) | null>(null);

  // A drag ends if the headers go away mid-drag.
  useEffect(() => () => stopReorder.current?.(), []);

  const startReorder = (track: TrackView, from: number, event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    stopReorder.current?.();
    let to = from;
    const middles = () =>
      Array.from(listRef.current?.children ?? [])
        .filter((child) => child.classList.contains("track-header"))
        .map((child) => {
          const box = child.getBoundingClientRect();
          return (box.top + box.bottom) / 2;
        });
    const move = (move: globalThis.PointerEvent) => {
      to = dropIndex(middles(), from, move.clientY);
      setReorder({ from, to });
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("keydown", key);
      stopReorder.current = null;
      setReorder(null);
    };
    const end = () => {
      stop();
      if (to !== from) onMove(track.id, to);
    };
    const key = (key: KeyboardEvent) => {
      if (key.key === "Escape") stop();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("keydown", key);
    stopReorder.current = stop;
  };

  // Where the dragged header would land: a line above the header it would
  // go before, or below the one it would go after.
  const dropMark = (index: number): string => {
    if (!reorder || reorder.to === reorder.from || index !== reorder.to) return "";
    return reorder.to < reorder.from ? " drop-above" : " drop-below";
  };

  const full = project.tracks.length >= project.maxTracks;
  return (
    <section className="track-headers" aria-label="Tracks">
      <ol ref={listRef}>
        {project.tracks.map((track, index) => (
          <TrackHeader
            key={track.id}
            track={track}
            limits={project.mixerLimits}
            selected={track.id === selected}
            dragging={reorder?.from === index}
            className={dropMark(index)}
            levels={levels}
            onSelect={() => onSelect(track.id)}
            onMixer={(mixer, gesture) => onMixer(track.id, mixer, gesture)}
            onSoloAlone={() => onSoloAlone(track.id)}
            onStartReorder={(event) => startReorder(track, index, event)}
          />
        ))}
      </ol>
      <button
        type="button"
        className="add-track"
        disabled={full}
        title={full ? `A project has at most ${project.maxTracks} tracks` : undefined}
        onClick={onAdd}
      >
        + Add track
      </button>
    </section>
  );
}

interface HeaderProps {
  track: TrackView;
  limits: MixerLimits;
  selected: boolean;
  dragging: boolean;
  className: string;
  levels: TrackLevels;
  onSelect: () => void;
  onMixer: (mixer: MixerView, gesture?: number) => void;
  onSoloAlone: () => void;
  onStartReorder: (event: PointerEvent) => void;
}

/** One track's header. It shows the mixer strip Rust last sent. */
function TrackHeader({
  track,
  limits,
  selected,
  dragging,
  className,
  levels,
  onSelect,
  onMixer,
  onSoloAlone,
  onStartReorder,
}: HeaderProps) {
  const volumeGesture = useGesture();
  const panGesture = useGesture();
  const { mixer, name } = track;
  const clamp = ([min, max]: [number, number], value: number) =>
    Math.min(max, Math.max(min, value));
  return (
    <li
      className={`track-header${selected ? " selected" : ""}${dragging ? " dragging" : ""}${className}`}
      aria-label={name}
      aria-current={selected ? "true" : undefined}
      onPointerDown={onSelect}
    >
      <div className="track-title">
        <span className="track-name" title="Drag to reorder" onPointerDown={onStartReorder}>
          {name}
        </span>
        <button
          type="button"
          className="mute"
          aria-label={`Mute ${name}`}
          aria-pressed={mixer.mute}
          onClick={() => onMixer({ ...mixer, mute: !mixer.mute })}
        >
          M
        </button>
        <button
          type="button"
          className="solo"
          aria-label={`Solo ${name}`}
          aria-pressed={mixer.solo}
          title="⌥-click to solo on its own"
          onClick={(event) =>
            event.altKey ? onSoloAlone() : onMixer({ ...mixer, solo: !mixer.solo })
          }
        >
          S
        </button>
      </div>
      <label className="track-setting">
        <span>Vol</span>
        <input
          type="range"
          aria-label={`${name} volume`}
          min={limits.volumeDb[0]}
          max={limits.volumeDb[1]}
          step={0.5}
          value={clamp(limits.volumeDb, mixer.volumeDb)}
          aria-valuetext={formatDb(mixer.volumeDb)}
          onPointerDown={volumeGesture.start}
          onChange={(event) =>
            onMixer(
              { ...mixer, volumeDb: event.currentTarget.valueAsNumber },
              volumeGesture.current(),
            )
          }
        />
        <output>{formatDb(mixer.volumeDb)}</output>
      </label>
      <label className="track-setting">
        <span>Pan</span>
        <input
          type="range"
          aria-label={`${name} pan`}
          min={limits.pan[0]}
          max={limits.pan[1]}
          step={0.01}
          value={clamp(limits.pan, mixer.pan)}
          aria-valuetext={formatPan(mixer.pan)}
          onPointerDown={panGesture.start}
          onChange={(event) =>
            onMixer({ ...mixer, pan: event.currentTarget.valueAsNumber }, panGesture.current())
          }
        />
        <output>{formatPan(mixer.pan)}</output>
      </label>
      <Meter level={levels.level(track.id)} label={`${name} meter`} width={METER_WIDTH} />
    </li>
  );
}
