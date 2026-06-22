// Maps a timeline track key to the real dub setting it controls. Used for
// track selection, the track inspector, and the gutter eye (enable/disable).

export type TrackAudio = "video" | "original" | "vn" | "subSrc" | "sub" | null;

/**
 * Which real setting a track drives:
 * - V1 → video frames (off = audio-only export)
 * - A1 → original audio (volume)
 * - TTS → VN dub audio (volume)
 * - SZH → source subtitle (bilingual flag)
 * - SVI → burned VN subtitle
 */
export function trackAudioKind(key: string): TrackAudio {
  if (key === "V1") return "video";
  if (key === "A1") return "original";
  if (key === "TTS") return "vn";
  if (key === "SZH") return "subSrc";
  if (key === "SVI") return "sub";
  return null; // A2 (extra music) — no export control
}

/** Track-level controls wired to the project's real settings. */
export interface TrackCtl {
  kindOf: (key: string) => TrackAudio;
  /** Is the track included in preview + export? */
  enabled: (key: string) => boolean;
  /** Toggle the track on/off (volume↔0 for audio, burn flag for subs). */
  toggle: (key: string) => void;
  /** Does this track have a meaningful enable/disable control? */
  hasEye: (key: string) => boolean;
  /** Current volume 0..1 for an audio track (original/vn), else null. */
  volume: (key: string) => number | null;
  /** Set an audio track's volume 0..1 (persists). */
  setVolume: (key: string, v: number) => void;
}
