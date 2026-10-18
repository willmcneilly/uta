import { useEffect } from "react";
import type { DemoSongItem } from "../backend";
import { Key } from "../design/Key";
import "./LoadDemoSong.css";

interface Props {
  song: DemoSongItem;
  onLoad: () => void;
  onCancel: () => void;
}

/**
 * Develop → Load Demo Song: asks first, because the song replaces the whole
 * project. Esc or Cancel changes nothing.
 */
export function LoadDemoSong({ song, onLoad, onCancel }: Props) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);

  return (
    <div className="demo-song-backdrop">
      <section
        className="demo-song"
        role="dialog"
        aria-modal="true"
        aria-labelledby="demo-song-title"
      >
        <h2 id="demo-song-title">Load {song.name}</h2>
        <p>This replaces the current project. Undo brings it back.</p>
        <div className="demo-song-actions">
          <Key onClick={onCancel}>Cancel</Key>
          <Key primary onClick={onLoad} autoFocus>
            Replace and Play
          </Key>
        </div>
      </section>
    </div>
  );
}
