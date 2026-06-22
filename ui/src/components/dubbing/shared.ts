// Shared helpers for the video-dubbing UI.

import type { DubProject, DubSegment, DubSettings } from "../../studioApi";

export type VoiceOpt = { value: string; label: string };

/** A full settings payload from a project + overrides (every save sends all fields). */
export function settingsOf(p: DubProject, over?: Partial<DubSettings>): DubSettings {
  return {
    name: p.name,
    gemini_model: p.gemini_model,
    original_volume: p.original_volume,
    vn_volume: p.vn_volume,
    speed_cap: p.speed_cap,
    burn_subtitles: p.burn_subtitles,
    blur_subtitle: p.blur_subtitle,
    blur_x: p.blur_x,
    blur_y: p.blur_y,
    blur_w: p.blur_w,
    blur_h: p.blur_h,
    sub_y: p.sub_y,
    sub_size: p.sub_size,
    sub_color: p.sub_color,
    sub_bilingual: p.sub_bilingual,
    video_enabled: p.video_enabled,
    ...over,
  };
}

export function clock(t: number): string {
  const s = Math.max(0, t);
  return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

export function fmtDuration(sec: number | null): string {
  if (!sec && sec !== 0) return "—";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}

export function fmtBytes(n: number | null): string {
  if (!n) return "—";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 && i > 0 ? 1 : 0)} ${u[i]}`;
}

export function genderLabel(g: string | null | undefined): string {
  return g === "male" ? "♂ Nam" : g === "female" ? "♀ Nữ" : g === "child" ? "Trẻ em" : "— ?";
}

/** Friendly "Người N" name from a "SPEAKER_0N" label. */
export function speakerName(speaker: string): string {
  return speaker.replace(/^SPEAKER_0*/, "Người ").replace(/^SPEAKER_/, "Người ");
}

function vttTime(x: number): string {
  const ms = Math.max(0, Math.round(x * 1000));
  const p = (n: number, l = 2) => String(n).padStart(l, "0");
  return `${p(Math.floor(ms / 3600000))}:${p(Math.floor(ms / 60000) % 60)}:${p(Math.floor(ms / 1000) % 60)}.${p(ms % 1000, 3)}`;
}

/** WebVTT from segments, cues positioned vertically to mirror the burned MarginV. */
export function buildVtt(segs: DubSegment[], lineFrac: number): string {
  const line = `line:${Math.round(Math.min(1, Math.max(0, lineFrac)) * 100)}% position:50%`;
  let out = "WEBVTT\n\n";
  segs.forEach((s, i) => {
    if (!s.text_vi.trim()) return;
    out += `${i + 1}\n${vttTime(s.start_s)} --> ${vttTime(Math.max(s.end_s, s.start_s + 0.1))} ${line}\n${s.text_vi}\n\n`;
  });
  return out;
}
