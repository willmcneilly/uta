import { type ReactNode, useState } from "react";
import { moveFocus, usePopover } from "./popover";
import { type TailSettings, secondsAt, tailAt, xOf } from "./tail";
import "./PresetKey.css";

// The preset key on a panel's spine, under its name, opening a menu of the
// panel's factory sounds: the same on the Reverb and the synth, so it's
// there when the panel is folded. Its glyph is a row of preset buttons with
// the middle one held down, what it opens and nothing else. The menu is the
// + menu's popover. It names no sound: choosing one sets every setting, and
// nothing remembers which it was. The Reverb's lists its sounds by mode,
// each with a small drawing of its tail, its name and the Mix it suggests,
// with the pointed-at sound's brief at its foot. provisional: D-39

/** One sound in a preset menu. */
export interface PresetItem {
  /** Its key, as the call that chooses it takes it. */
  value: string;
  label: string;
  /** The heading it's listed under, if the menu has them: its mode. Items with the same heading are listed together. */
  group?: string;
  /** What's shown at its right: its suggested Mix. */
  aside?: string;
  drawing?: ReactNode;
  /** Its brief, shown at the menu's foot while it's pointed at. */
  about?: string;
}

interface Props {
  /** What the key and its menu are called: "Reverb sounds". */
  label: string;
  items: PresetItem[];
  /** The menu's headings for its two columns, if it has them. */
  heading?: { name: string; aside: string };
  /** The menu's last line, under the pointed-at sound's brief. */
  foot?: string;
  onChoose: (value: string) => void;
}

/** How far below the top of the key the menu opens, in CSS pixels: under it. */
const MENU_DROP = 24;

/** The glyph: three preset buttons, the middle one held down. */
function Glyph() {
  return (
    <svg viewBox="0 0 16 16" width={16} height={16} aria-hidden="true">
      <rect className="up" x={1.5} y={4.5} width={3.5} height={7} rx={1} />
      <rect className="down" x={6.25} y={4.5} width={3.5} height={7} rx={1} />
      <rect className="up" x={11} y={4.5} width={3.5} height={7} rx={1} />
    </svg>
  );
}

/** A menu row's drawing's size, in CSS pixels. */
const MINI_WIDTH = 40;
const MINI_HEIGHT = 12;

/**
 * A Reverb sound's tail, small, on the panel drawing's time scale: its
 * loudest band's level from the note to the scale's end. provisional: D-39
 */
export function MiniTail({ settings }: { settings: TailSettings }) {
  const steps = 32;
  const points = Array.from({ length: steps + 1 }, (_, i) => {
    const x = (i / steps) * MINI_WIDTH;
    const level = tailAt(settings, secondsAt(x, MINI_WIDTH)).all;
    return `${x.toFixed(1)},${(MINI_HEIGHT - level * MINI_HEIGHT).toFixed(1)}`;
  });
  const start = xOf(settings.predelaySeconds, MINI_WIDTH);
  return (
    <svg
      className="mini-tail"
      viewBox={`0 0 ${MINI_WIDTH} ${MINI_HEIGHT}`}
      width={MINI_WIDTH}
      height={MINI_HEIGHT}
      aria-hidden="true"
    >
      <path d={`M${start.toFixed(1)},${MINI_HEIGHT} L${points.join(" L")} L${MINI_WIDTH},${MINI_HEIGHT} Z`} />
    </svg>
  );
}

export function PresetKey({ label, items, heading, foot, onChoose }: Props) {
  const [open, setOpen] = useState(false);
  const [pointed, setPointed] = useState<string | null>(null);
  const { area, key, menu } = usePopover(open, () => setOpen(false), MENU_DROP);

  // Each heading once, in the order its first item comes, with every item
  // under it in order, wherever it is in the list.
  const groups = items.reduce<{ name?: string; items: PresetItem[] }[]>((groups, item) => {
    const found = groups.find((group) => group.name === item.group);
    if (found) found.items.push(item);
    else groups.push({ name: item.group, items: [item] });
    return groups;
  }, []);
  const about = items.find((item) => item.value === pointed)?.about;

  return (
    <div
      className="preset"
      ref={area}
      // The spine drags its panel; the key and its menu don't.
      onPointerDown={(event) => event.stopPropagation()}
    >
      <button
        ref={key}
        type="button"
        className="preset-key"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={label}
        title={label}
        onClick={() => setOpen(!open)}
      >
        <Glyph />
      </button>
      {open && (
        <div
          ref={menu}
          className="preset-menu"
          role="menu"
          aria-label={label}
          onKeyDown={(event) => moveFocus(area, event)}
        >
          {heading && (
            <div className="preset-heading" aria-hidden="true">
              <span>{heading.name}</span>
              <span>{heading.aside}</span>
            </div>
          )}
          {groups.map((group, index) => (
            <div key={group.name ?? index} role="group" aria-label={group.name}>
              {group.name && (
                <div className="preset-group" aria-hidden="true">
                  {group.name}
                </div>
              )}
              {group.items.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  role="menuitem"
                  tabIndex={-1}
                  onPointerEnter={() => setPointed(item.value)}
                  onFocus={() => setPointed(item.value)}
                  onClick={() => {
                    setOpen(false);
                    onChoose(item.value);
                  }}
                >
                  {item.drawing}
                  <span className="preset-name">{item.label}</span>
                  {item.aside && <span className="preset-aside">{item.aside}</span>}
                </button>
              ))}
            </div>
          ))}
          {(about || foot) && (
            <div className="preset-foot">
              {about && <p>{about}</p>}
              {foot && <p className="preset-mix">{foot}</p>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
