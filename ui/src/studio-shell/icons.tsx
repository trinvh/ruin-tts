// Shared SVG icon set for the Beesoft Studio shell + Video Studio editor.
// Paths are lifted directly from the design handoff so glyphs match the mockup.

export type IconName =
  | "plus" | "close" | "chevronR" | "chevronD" | "home" | "film" | "wave"
  | "flows" | "runs" | "settings" | "api" | "play" | "pause" | "toStart"
  | "toEnd" | "undo" | "redo" | "magnet" | "runAll" | "export" | "eye"
  | "split" | "trash" | "zoomIn" | "zoomOut" | "maximize" | "check" | "lock"
  | "image" | "music" | "speaker" | "subtitle";

// Stroke-based icons: array of <path d> strings.
const STROKE: Partial<Record<IconName, string[]>> = {
  plus: ["M12 5v14M5 12h14"],
  close: ["M6 6l12 12M18 6 6 18"],
  chevronR: ["m9 6 6 6-6 6"],
  chevronD: ["m6 9 6 6 6-6"],
  home: ["M3 11l9-7 9 7", "M5 10v9h14v-9"],
  film: ["M4 5h16v14H4z", "M4 9h16", "M9 5v14", "M15 5v14"],
  wave: ["M11 5 6 9H3v6h3l5 4z", "M15.5 8.5a5 5 0 0 1 0 7", "M18.5 6a8 8 0 0 1 0 12"],
  flows: ["M3 4h6v5H3z", "M15 15h6v5h-6z", "M6 9v4a3 3 0 0 0 3 3h6"],
  runs: ["M3 12h4l3 8 4-16 3 8h4"],
  settings: [
    "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6",
    "M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.5-2.4 1a7 7 0 0 0-1.7-1L14 3h-4l-.8 2.5a7 7 0 0 0-1.7 1l-2.4-1-2 3.5 2 1.5a7 7 0 0 0 0 2l-2 1.5 2 3.5 2.4-1a7 7 0 0 0 1.7 1L10 21h4l.8-2.5a7 7 0 0 0 1.7-1l2.4 1 2-3.5-2-1.5a7 7 0 0 0 .1-1z",
  ],
  api: ["M8 8 4 12l4 4", "M16 8l4 4-4 4", "M13.5 6l-3 12"],
  toEnd: [], // rendered as fill below
  undo: ["M9 14 4 9l5-5", "M4 9h11a5 5 0 0 1 0 10h-4"],
  redo: ["m15 14 5-5-5-5", "M20 9H9a5 5 0 0 0 0 10h4"],
  export: ["M12 15V3", "m7 8 5-5 5 5", "M5 13v6a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-6"],
  eye: ["M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z", "M12 12m-2.5 0a2.5 2.5 0 1 0 5 0 2.5 2.5 0 1 0-5 0"],
  split: ["M6 6m-3 0a3 3 0 1 0 6 0 3 3 0 1 0-6 0", "M6 18m-3 0a3 3 0 1 0 6 0 3 3 0 1 0-6 0", "M20 4 8.12 15.88M14.47 14.48 20 20M8.12 8.12 12 12"],
  trash: ["M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13"],
  zoomIn: ["M11 11m-7 0a7 7 0 1 0 14 0 7 7 0 1 0-14 0", "m21 21-4.3-4.3", "M11 8v6M8 11h6"],
  zoomOut: ["M11 11m-7 0a7 7 0 1 0 14 0 7 7 0 1 0-14 0", "m21 21-4.3-4.3", "M8 11h6"],
  maximize: ["M8 3H5a2 2 0 0 0-2 2v3", "M16 3h3a2 2 0 0 1 2 2v3", "M21 16v3a2 2 0 0 1-2 2h-3", "M3 16v3a2 2 0 0 0 2 2h3"],
  check: ["m5 12 5 5 9-10"],
  lock: ["M5 11h14v9H5z", "M8 11V8a4 4 0 0 1 8 0v3"],
  image: ["M4 4h16v16H4z", "M8 11l3 3 5-6"],
  music: ["M9 18V6l10-2v12", "M9 18a3 3 0 1 1-2-2.8", "M19 16a3 3 0 1 1-2-2.8"],
  speaker: ["M11 5 6 9H3v6h3l5 4z", "M16 9a4 4 0 0 1 0 6"],
  subtitle: ["M5 5h14v10H8l-3 3z"],
};

// Fill-based icons (solid shapes): array of <path d> strings.
const FILL: Partial<Record<IconName, string[]>> = {
  play: ["M7 5v14l12-7z"],
  pause: ["M6.5 5h4v14h-4z", "M13.5 5h4v14h-4z"],
  toStart: ["M18 5.5v13L9 12z", "M6 5.5h2.2v13H6z"],
  toEnd: ["M6 5.5v13L15 12z", "M15.8 5.5H18v13h-2.2z"],
  magnet: ["M5 3a1 1 0 0 0-1 1v7a8 8 0 0 0 16 0V4a1 1 0 0 0-1-1h-3a1 1 0 0 0-1 1v7a3 3 0 0 1-6 0V4a1 1 0 0 0-1-1z"],
  runAll: ["M5 4.5 13 12l-8 7.5zM13 4.5 21 12l-8 7.5z"],
};

interface IconProps {
  name: IconName;
  size?: number;
  width?: number;
  stroke?: number;
  color?: string;
  style?: React.CSSProperties;
}

/** Render a named icon. Uses currentColor by default so callers set colour via `color` on the wrapper. */
export function Icon({ name, size = 16, width, stroke = 1.7, color, style }: IconProps) {
  const w = width ?? size;
  const fillPaths = FILL[name];
  if (fillPaths) {
    return (
      <svg viewBox="0 0 24 24" width={w} height={size} fill={color ?? "currentColor"} style={style}>
        {fillPaths.map((d, i) => (
          <path key={i} d={d} />
        ))}
      </svg>
    );
  }
  const paths = STROKE[name] ?? [];
  return (
    <svg
      viewBox="0 0 24 24"
      width={w}
      height={size}
      fill="none"
      stroke={color ?? "currentColor"}
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={style}
    >
      {paths.map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  );
}
