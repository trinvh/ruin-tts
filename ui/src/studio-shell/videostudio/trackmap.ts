// Maps a timeline track key to the real dub setting it controls. Used for
// track selection, the track inspector, and the gutter eye (enable/disable).

export type TrackAudio = "original" | "vn" | "sub" | null;

/** Which real setting a track drives: A1→original audio, TTS→VN dub, SVI→burned subs. */
export function trackAudioKind(key: string): TrackAudio {
  if (key === "A1") return "original";
  if (key === "TTS") return "vn";
  if (key === "SVI") return "sub";
  return null; // V1 (video), SZH (source subs), A2 — no export control
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
