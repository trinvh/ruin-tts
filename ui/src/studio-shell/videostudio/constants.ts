// Sample data + pure helpers for the Video Studio editor, ported from the design.

import { C } from "../theme";
import type { AudioKind, Clip, DubLine, DubVidState, StudioState, SubLang } from "./types";

export const THUMB_VIDEO = "/studio/clip2.svg";
export const THUMB_VIDEO_ALT = "/studio/clip1.svg";
export const THUMB_IMAGE = "/studio/clip4.svg";

export const DUB: DubLine[] = [
  { t: 0.2, d: 1.3, zh: "房子修好了", vi: "Nhà xong rồi." },
  { t: 1.6, d: 1.1, zh: "养老一层就够了", vi: "Một tầng là đủ." },
  { t: 2.8, d: 1.4, zh: "居住方便不用爬楼", vi: "Ở tiện, không leo lầu." },
  { t: 4.3, d: 1.0, zh: "养老很舒服", vi: "Dưỡng lão thoải mái." },
  { t: 5.4, d: 1.2, zh: "做了四个卧室", vi: "Có bốn phòng ngủ." },
  { t: 6.7, d: 1.0, zh: "一间客堂", vi: "Một phòng khách." },
  { t: 7.8, d: 0.9, zh: "一间厨房", vi: "Một phòng bếp." },
  { t: 8.8, d: 1.0, zh: "两个卫生间", vi: "Hai nhà vệ sinh." },
  { t: 9.9, d: 0.7, zh: "一个院子", vi: "Một cái sân." },
  { t: 10.6, d: 0.4, zh: "很宽敞", vi: "Rất rộng rãi." },
];

export const SUBC = ["#ffffff", "#FFE082", "#7FE6C6", "#FF7CA3"];

export const DEFAULT_VOICE = "Mỹ Duyên — nữ, miền Nam";

export function defDub(): DubVidState {
  return {
    pipe: { tach: "idle", phan: "idle", dich: "idle", tts: "idle" },
    inserted: { szh: false, svi: false, tts: false },
    voice: DEFAULT_VOICE,
  };
}

export function initialState(): StudioState {
  return {
    tab: "dub",
    dub: {},
    aspect: "9:16",
    snap: true,
    zoom: 100,
    playing: false,
    playhead: 0.6,
    sel: "vid",
    subStyle: { size: 30, color: "#ffffff", pos: 80, bg: true, bilingual: false },
    clips: [
      { id: "vid", track: "V1", type: "video", name: "养老_1080p.mp4", start: 0, dur: 11, scale: 104, posY: 0, opacity: 100, vol: 100, bri: 0, con: 0, sat: 0, thumb: THUMB_VIDEO },
      { id: "aud", track: "A1", type: "audio", kind: "orig", name: "Âm thanh gốc", srcVideo: "vid", start: 0, dur: 11, vol: 100, speed: 100, fadeIn: 0, fadeOut: 0 },
    ],
  };
}

/** Total timeline duration (seconds), min 11, ceil of furthest clip end. */
export function totalDur(clips: Clip[]): number {
  let m = 11;
  for (const c of clips) m = Math.max(m, c.start + c.dur);
  return Math.ceil(m);
}

/** mm:ss:ff (30fps) */
export function fmt(t: number): string {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  const f = Math.floor((t % 1) * 30);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(m)}:${p(s)}:${p(f)}`;
}

/** Deterministic pseudo-waveform bar heights. */
export function bars(seed: number, n: number): { h: number }[] {
  const a: { h: number }[] = [];
  for (let i = 0; i < n; i++) {
    a.push({ h: Math.round(4 + Math.abs(Math.sin((i + seed) * 0.7) * Math.cos((i + seed) * 0.21)) * 22) });
  }
  return a;
}

export const TRACK_ORDER: Record<string, number> = { V1: 0, IMG: 1, A1: 2, A2: 3, TTS: 4, SZH: 5, SVI: 6 };

export function trackLabel(k: string, tachDone: boolean): string {
  return (
    {
      V1: "V1 · Video",
      IMG: "Ảnh",
      A1: tachDone ? "Giọng gốc" : "Âm thanh",
      A2: "Nhạc nền",
      TTS: "Lồng tiếng Việt",
      SZH: "Phụ đề gốc",
      SVI: "Phụ đề Việt",
    }[k] ?? k
  );
}

export function trackDot(k: string): string {
  return (
    { V1: C.coral, IMG: C.blue, A1: C.blue, A2: C.teal, TTS: C.purple, SZH: "#5a6b8c", SVI: C.purple }[k] ?? C.muted
  );
}

export interface ClipColors {
  bg: string;
  bgImg: string;
  scrim: string;
  textColor: string;
  isAudio: boolean;
  wave: string;
}

export function clipColors(c: Clip): ClipColors {
  if (c.type === "video" || c.type === "image") {
    return {
      bg: c.type === "video" ? "#2b2330" : C.panel3,
      bgImg: c.thumb ? `url('${c.thumb}')` : "none",
      scrim: "linear-gradient(180deg,rgba(15,16,22,.05),rgba(15,16,22,.6))",
      textColor: "#fff",
      isAudio: false,
      wave: "",
    };
  }
  if (c.type === "audio") {
    const kind: AudioKind = c.kind ?? "orig";
    const w = kind === "music" ? C.teal : kind === "tts" ? C.purpleLt : C.blue;
    const bgc = kind === "music" ? "rgba(80,209,170,.14)" : kind === "tts" ? "rgba(146,136,224,.16)" : "rgba(101,176,246,.13)";
    return { bg: bgc, bgImg: "none", scrim: "transparent", textColor: w, isAudio: true, wave: w };
  }
  const lang: SubLang = c.lang ?? "vi";
  const sc = lang === "vi" ? { bg: "rgba(146,136,224,.18)", t: "#cfc8f5" } : { bg: "rgba(101,176,246,.14)", t: "#9fccfa" };
  return { bg: sc.bg, bgImg: "none", scrim: "transparent", textColor: sc.t, isAudio: false, wave: "" };
}
