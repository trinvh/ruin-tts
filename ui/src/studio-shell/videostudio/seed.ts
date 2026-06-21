import type { DubDetail } from "../../studioApi";
import type { Clip } from "./types";

/**
 * Build the timeline clips for the editor from a real dub project: the source
 * video + original audio, plus subtitle (source/Vietnamese) and TTS tracks as
 * the pipeline produces them. The timeline is a faithful visualisation — drag /
 * trim stay client-side; the authoritative data is the project itself.
 */
export function buildClips(detail: DubDetail | null, duration: number): Clip[] {
  const dur = duration > 0 ? duration : 11;
  if (!detail) {
    return [{ id: "vid", track: "V1", type: "video", name: "Video", start: 0, dur, scale: 100, posY: 0, opacity: 100, vol: 100, bri: 0, con: 0, sat: 0 }];
  }
  const p = detail.project;
  const segs = detail.segments;
  const extracted = ["extracted", "analyzed", "translated", "synthesized", "built", "done"].includes(p.status);
  const hasSrc = segs.length > 0;
  const hasVi = segs.some((s) => s.text_vi.trim().length > 0);
  const hasTts = segs.some((s) => !!s.tts_path) || ["synthesized", "built", "done"].includes(p.status);

  const clips: Clip[] = [
    { id: "vid", track: "V1", type: "video", name: p.name, start: 0, dur, scale: 100, posY: 0, opacity: 100, vol: Math.round(p.original_volume * 100), bri: 0, con: 0, sat: 0 },
    { id: "aud", track: "A1", type: "audio", kind: extracted ? "vocals" : "orig", name: extracted ? "Giọng gốc" : "Âm thanh gốc", srcVideo: "vid", start: 0, dur, vol: Math.round(p.original_volume * 100), speed: 100, fadeIn: 0, fadeOut: 0 },
  ];

  const segDur = (s: { start_s: number; end_s: number }) => Math.max(0.2, s.end_s - s.start_s);
  if (hasSrc) {
    for (const s of segs) clips.push({ id: "szh_" + s.id, track: "SZH", type: "sub", lang: "zh", name: s.text_src || "(…)", text: s.text_src, srcVideo: "vid", start: s.start_s, dur: segDur(s) });
  }
  if (hasVi) {
    for (const s of segs) clips.push({ id: "svi_" + s.id, track: "SVI", type: "sub", lang: "vi", name: s.text_vi || "(chưa dịch)", text: s.text_vi, srcVideo: "vid", start: s.start_s, dur: segDur(s) });
  }
  if (hasTts) {
    for (const s of segs) clips.push({ id: "tts_" + s.id, track: "TTS", type: "audio", kind: "tts", name: s.text_vi || "TTS", srcVideo: "vid", start: s.start_s, dur: segDur(s), vol: 100, speed: 100, fadeIn: 0, fadeOut: 0 });
  }
  return clips;
}

/** Changes only when the timeline visualisation should be rebuilt. */
export function clipSignature(detail: DubDetail | null, duration: number): string {
  if (!detail) return `none:${duration}`;
  const p = detail.project;
  return `${p.status}:${Math.round(duration)}:${p.original_volume}:${detail.segments.map((s) => s.id + "=" + s.text_vi.length + "/" + s.text_src.length).join(",")}`;
}

/** Map a seeded subtitle clip id back to its segment id (or null). */
export function segIdOfClip(clipId: string): string | null {
  const m = /^(?:svi|szh|tts)_(.+)$/.exec(clipId);
  return m ? m[1] : null;
}
