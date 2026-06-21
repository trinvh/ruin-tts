// Video Studio editor types (front-end-only visual port).

export type ClipType = "video" | "audio" | "image" | "sub";
export type AudioKind = "orig" | "vocals" | "music" | "tts";
export type SubLang = "zh" | "vi";
export type PipeKey = "tach" | "phan" | "dich" | "tts";
export type PipeStatus = "idle" | "running" | "done";
export type Aspect = "9:16" | "1:1" | "16:9";

export interface Clip {
  id: string;
  track: string;
  type: ClipType;
  name: string;
  start: number;
  dur: number;
  // visual (video / image)
  scale?: number;
  posY?: number;
  opacity?: number;
  thumb?: string;
  bri?: number;
  con?: number;
  sat?: number;
  // image overlay position (% of preview)
  ox?: number;
  oy?: number;
  // audio
  kind?: AudioKind;
  srcVideo?: string;
  vol?: number;
  speed?: number;
  fadeIn?: number;
  fadeOut?: number;
  // subtitle
  lang?: SubLang;
  text?: string;
}

export interface DubVidState {
  pipe: Record<PipeKey, PipeStatus>;
  inserted: { szh: boolean; svi: boolean; tts: boolean };
  voice: string;
}

export interface SubStyle {
  size: number;
  color: string;
  pos: number;
  bg: boolean;
  bilingual: boolean;
}

export interface StudioState {
  tab: "media" | "dub";
  dub: Record<string, DubVidState>;
  aspect: Aspect;
  snap: boolean;
  zoom: number;
  playing: boolean;
  playhead: number;
  sel: string | null;
  subStyle: SubStyle;
  clips: Clip[];
}

export interface DubLine {
  t: number;
  d: number;
  zh: string;
  vi: string;
}
