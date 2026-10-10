import { type PointerEvent, type Ref, useEffect, useRef, useState } from "react";
import type { MixerLimits, MixerView, ProjectView, TrackView } from "./backend";
import { Slider } from "./design/Slider";
import { linearScale } from "./design/sliderScale";
import { Meter } from "./Meter";
import type { TrackLevels } from "./meterLevel";
import { ADD_TRACK_HEIGHT, RULER_HEIGHT, TRACK_HEIGHT } from "./timeline/viewport";
import { dropIndex, formatPan } from "./trackOrder";
import "./TrackHeaders.css";

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
  /** The scrolling element, which the timeline scrolls in step with its tracks. */
  scrollRef?: Ref<HTMLElement>;
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
 * its name to reorder the tracks. Each is as tall as its row on the
 * timeline, below a gap as tall as the timeline's ruler, so they line up.
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
  scrollRef,
}: Props) {
  const listRef = useRef<HTMLOListElement>(null);
  const [reorder, setReorder] = useState<Reorder | null>(null);
  const stopReorder = useRef<(() => void) | null>(null);
  // The drop reads the latest tracks: a project from Rust can arrive mid-drag.
  const tracks = useRef(project.tracks);
  useEffect(() => {
    tracks.current = project.tracks;
  }, [project.tracks]);

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
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      window.removeEventListener("keydown", key);
      stopReorder.current = null;
      setReorder(null);
    };
    const end = () => {
      stop();
      // Where the track is now, in case the tracks changed during the drag.
      const now = tracks.current.findIndex((other) => other.id === track.id);
      const index = Math.min(to, tracks.current.length - 1);
      if (now >= 0 && index !== now) onMove(track.id, index);
    };
    const key = (key: KeyboardEvent) => {
      if (key.key === "Escape") stop();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    // A drag the window loses, to ⌘-Tab say, ends without moving anything.
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
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
    <section className="track-headers" aria-label="Tracks" ref={scrollRef}>
      <div className="track-headers-ruler" style={{ height: RULER_HEIGHT }} />
      <ol ref={listRef}>
        {project.tracks.map((track, index) => (
          <TrackHeader
            key={track.id}
            track={track}
            limits={project.mixerLimits}
            defaults={project.mixerDefaults}
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
      <div className="add-track-row" style={{ height: ADD_TRACK_HEIGHT }}>
        <button
          type="button"
          className="add-track"
          disabled={full}
          title={full ? `A project has at most ${project.maxTracks} tracks` : undefined}
          onClick={onAdd}
        >
          + Add track
        </button>
      </div>
    </section>
  );
}

interface HeaderProps {
  track: TrackView;
  limits: MixerLimits;
  /** A new track's mixer strip, which double-click resets each slider to. */
  defaults: MixerView;
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
  defaults,
  selected,
  dragging,
  className,
  levels,
  onSelect,
  onMixer,
  onSoloAlone,
  onStartReorder,
}: HeaderProps) {
  const { mixer, name } = track;
  return (
    <li
      className={`track-header${selected ? " selected" : ""}${dragging ? " dragging" : ""}${className}`}
      aria-label={name}
      aria-current={selected ? "true" : undefined}
      style={{ height: TRACK_HEIGHT }}
      onPointerDown={onSelect}
      // Tabbing onto any of its controls selects the track too.
      onFocus={onSelect}
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
      <Slider
        className="track-setting"
        label="Vol"
        ariaLabel={`${name} volume`}
        value={mixer.volumeDb}
        defaultValue={defaults.volumeDb}
        scale={linearScale(...limits.volumeDb, 0.5)}
        format={formatDb}
        onChange={(volumeDb, gesture) => onMixer({ ...mixer, volumeDb }, gesture)}
      />
      <Slider
        className="track-setting"
        label="Pan"
        ariaLabel={`${name} pan`}
        value={mixer.pan}
        defaultValue={defaults.pan}
        scale={linearScale(...limits.pan, 0.01)}
        format={formatPan}
        onChange={(pan, gesture) => onMixer({ ...mixer, pan }, gesture)}
      />
      <Meter level={levels.level(track.id)} label={`${name} meter`} width={METER_WIDTH} />
    </li>
  );
}
