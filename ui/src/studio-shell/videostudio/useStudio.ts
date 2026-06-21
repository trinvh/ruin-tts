import { useCallback, useEffect, useRef, useState } from "react";
import { DUB, defDub, initialState, totalDur, THUMB_VIDEO_ALT, THUMB_IMAGE } from "./constants";
import type { Clip, DubVidState, PipeKey, StudioState } from "./types";

export type { StudioState } from "./types";

type Updater = (s: StudioState) => Partial<StudioState>;

export interface StudioActions {
  setTab: (t: "media" | "dub") => void;
  // drag / transport
  onMove: (e: PointerEvent) => void;
  onUp: () => void;
  imgDown: (e: React.PointerEvent) => void;
  clipDown: (id: string, mode: "move" | "l" | "r") => (e: React.PointerEvent) => void;
  rulerDown: (e: React.PointerEvent) => void;
  deselect: () => void;
  /** Select a whole track (sel becomes `track:<KEY>`). */
  selectTrack: (key: string) => void;
  togglePlay: () => void;
  toStart: () => void;
  setPrev: (el: HTMLElement | null) => void;
  setLane: (el: HTMLElement | null) => void;
  // dub
  getDub: (id: string) => DubVidState;
  patchDub: (id: string, fn: (d: DubVidState) => Partial<DubVidState>) => void;
  run: (key: PipeKey) => void;
  runAll: () => void;
  insertSubs: (vid: string, which: "szh" | "svi") => void;
  insertTts: (vid: string) => void;
  addVideo: () => void;
  addImage: () => void;
  addMusic: () => void;
  // edit
  setClipNum: (key: keyof Clip, value: number) => void;
  setClipText: (value: string) => void;
  resetColor: () => void;
  setSubNum: (key: "size" | "pos", value: number) => void;
  setSubColor: (color: string) => void;
  /** Seed subtitle style from the persisted project (size/color/bilingual). */
  seedSubStyle: (partial: Partial<StudioState["subStyle"]>) => void;
  toggleSubBg: () => void;
  toggleBil: () => void;
  setVoice: (v: string) => void;
  toggleSnap: () => void;
  setAspect: (a: StudioState["aspect"]) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  splitSel: () => void;
  delSel: () => void;
  /** Replace the timeline clips (used to seed the editor from a real project). */
  replaceClips: (clips: Clip[]) => void;
}

export interface StudioRefs {
  prev: React.MutableRefObject<HTMLElement | null>;
  lane: React.MutableRefObject<HTMLElement | null>;
}

export function useStudio(): { state: StudioState; actions: StudioActions; refs: StudioRefs } {
  const [state, setState] = useState<StudioState>(initialState);

  const stateRef = useRef(state);
  stateRef.current = state;

  const prevRef = useRef<HTMLElement | null>(null);
  const laneRef = useRef<HTMLElement | null>(null);
  const dragRef = useRef<{ id: string; mode: "move" | "l" | "r"; x: number } | null>(null);
  const seekRef = useRef(false);
  const imgDragRef = useRef<{ id: string; x: number; y: number; ox: number; oy: number } | null>(null);
  const playTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeouts = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  const update = useCallback((u: Updater) => setState((s) => ({ ...s, ...u(s) })), []);
  const TT = () => totalDur(stateRef.current.clips);

  // ── drag ──
  const onMove = useCallback((e: PointerEvent) => {
    if (imgDragRef.current && prevRef.current) {
      const r = prevRef.current.getBoundingClientRect();
      const d = imgDragRef.current;
      const ox = d.ox + ((e.clientX - d.x) / r.width) * 100;
      const oy = d.oy + ((e.clientY - d.y) / r.height) * 100;
      update((s) => ({
        clips: s.clips.map((c) => (c.id === d.id ? { ...c, ox: Math.max(0, Math.min(66, ox)), oy: Math.max(0, Math.min(88, oy)) } : c)),
      }));
      return;
    }
    if (!laneRef.current) return;
    const rect = laneRef.current.getBoundingClientRect();
    const w = rect.width;
    const tt = TT();
    if (seekRef.current) {
      update(() => ({ playhead: Math.max(0, Math.min(tt, ((e.clientX - rect.left) / w) * tt)) }));
      return;
    }
    const drag = dragRef.current;
    if (!drag) return;
    const dsec = ((e.clientX - drag.x) / w) * tt;
    if (Math.abs(dsec) < 0.0005) return;
    drag.x = e.clientX;
    update((s) => {
      const targets = s.snap
        ? [0, tt, s.playhead].concat(s.clips.filter((c) => c.id !== drag.id).flatMap((c) => [c.start, c.start + c.dur]))
        : [];
      const snap = (v: number) => {
        if (!s.snap) return v;
        let best = v;
        let bd = 0.2;
        for (const t of targets) {
          const dd = Math.abs(t - v);
          if (dd < bd) {
            bd = dd;
            best = t;
          }
        }
        return best;
      };
      return {
        clips: s.clips.map((c) => {
          if (c.id !== drag.id) return c;
          let start = c.start;
          let dur = c.dur;
          if (drag.mode === "move") {
            let ns = Math.max(0, Math.min(tt - dur, start + dsec));
            const sS = snap(ns);
            const sE = snap(ns + dur);
            ns = Math.abs(sS - ns) <= Math.abs(sE - (ns + dur)) ? sS : sE - dur;
            start = Math.max(0, Math.min(tt - dur, ns));
          } else if (drag.mode === "l") {
            const ns = Math.max(0, Math.min(start + dur - 0.3, snap(start + dsec)));
            dur = dur + (start - ns);
            start = ns;
          } else {
            const ne = Math.max(start + 0.3, Math.min(tt, snap(start + dur + dsec)));
            dur = ne - start;
          }
          return { ...c, start, dur };
        }),
      };
    });
  }, [update]);

  const onUp = useCallback(() => {
    dragRef.current = null;
    seekRef.current = false;
    imgDragRef.current = null;
  }, []);

  useEffect(() => {
    const move = (e: PointerEvent) => onMove(e);
    const up = () => onUp();
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    const tos = timeouts.current;
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (playTimer.current) clearInterval(playTimer.current);
      Object.values(tos).forEach(clearTimeout);
    };
  }, [onMove, onUp]);

  const imgDown = useCallback((e: React.PointerEvent) => {
    e.stopPropagation();
    const img = stateRef.current.clips.find((c) => c.type === "image");
    if (!img) return;
    imgDragRef.current = { id: img.id, x: e.clientX, y: e.clientY, ox: img.ox ?? 62, oy: img.oy ?? 8 };
    update(() => ({ sel: img.id }));
  }, [update]);

  const clipDown = useCallback(
    (id: string, mode: "move" | "l" | "r") => (e: React.PointerEvent) => {
      e.stopPropagation();
      dragRef.current = { id, mode, x: e.clientX };
      update(() => ({ sel: id }));
    },
    [update],
  );

  const rulerDown = useCallback((e: React.PointerEvent) => {
    seekRef.current = true;
    if (!laneRef.current) return;
    const r = laneRef.current.getBoundingClientRect();
    const tt = TT();
    update(() => ({ playhead: Math.max(0, Math.min(tt, ((e.clientX - r.left) / r.width) * tt)) }));
  }, [update]);

  const deselect = useCallback(() => update(() => ({ sel: null })), [update]);
  const selectTrack = useCallback((key: string) => update(() => ({ sel: "track:" + key })), [update]);

  const togglePlay = useCallback(() => {
    if (stateRef.current.playing) {
      if (playTimer.current) clearInterval(playTimer.current);
      update(() => ({ playing: false }));
    } else {
      playTimer.current = setInterval(() => {
        update((s) => ({ playhead: s.playhead >= totalDur(s.clips) ? 0 : s.playhead + 0.05 }));
      }, 50);
      update(() => ({ playing: true }));
    }
  }, [update]);

  const toStart = useCallback(() => update(() => ({ playhead: 0 })), [update]);

  // ── dub ──
  const getDub = useCallback((id: string) => stateRef.current.dub[id] ?? defDub(), []);
  const patchDub = useCallback(
    (id: string, fn: (d: DubVidState) => Partial<DubVidState>) =>
      update((s) => {
        const d = s.dub[id] ?? defDub();
        return { dub: { ...s.dub, [id]: { ...d, ...fn(d) } } };
      }),
    [update],
  );

  const dubVidId = () => {
    const s = stateRef.current;
    const sel = s.clips.find((c) => c.id === s.sel);
    return sel && sel.type === "video" ? sel.id : null;
  };

  const splitAudio = useCallback((vid: string) => {
    update((s) => {
      const v = s.clips.find((c) => c.id === vid);
      if (!v) return {};
      let clips = s.clips.map((c) => (c.kind === "orig" && c.srcVideo === vid ? { ...c, kind: "vocals" as const, name: "Giọng gốc" } : c));
      if (!clips.find((c) => c.id === "mus_" + vid)) {
        clips = [...clips, { id: "mus_" + vid, track: "A2", type: "audio", kind: "music", name: "Nhạc nền", srcVideo: vid, start: v.start, dur: v.dur, vol: 55, speed: 100, fadeIn: 0, fadeOut: 0 }];
      }
      return { clips };
    });
  }, [update]);

  const insertSubs = useCallback((vid: string, which: "szh" | "svi") => {
    update((s) => {
      const d = s.dub[vid] ?? defDub();
      if (d.inserted[which]) return {};
      const v = s.clips.find((c) => c.id === vid);
      if (!v) return {};
      const lang = which === "szh" ? "zh" : "vi";
      const track = which === "szh" ? "SZH" : "SVI";
      const add: Clip[] = DUB.filter((l) => l.t < v.dur).map((l, i) => ({
        id: which + "_" + vid + "_" + i,
        track,
        type: "sub",
        lang,
        srcVideo: vid,
        name: lang === "zh" ? l.zh : l.vi,
        text: lang === "zh" ? l.zh : l.vi,
        start: v.start + l.t,
        dur: Math.min(v.dur - l.t, l.d),
      }));
      return { dub: { ...s.dub, [vid]: { ...d, inserted: { ...d.inserted, [which]: true } } }, clips: [...s.clips, ...add] };
    });
  }, [update]);

  const insertTts = useCallback((vid: string) => {
    update((s) => {
      const d = s.dub[vid] ?? defDub();
      if (d.inserted.tts) return {};
      const v = s.clips.find((c) => c.id === vid);
      if (!v) return {};
      const add: Clip[] = DUB.filter((l) => l.t < v.dur).map((l, i) => ({
        id: "tts_" + vid + "_" + i,
        track: "TTS",
        type: "audio",
        kind: "tts",
        srcVideo: vid,
        name: l.vi,
        start: v.start + l.t,
        dur: Math.min(v.dur - l.t, l.d * 1.05),
        vol: 100,
        speed: 100,
        fadeIn: 0,
        fadeOut: 0,
      }));
      return { dub: { ...s.dub, [vid]: { ...d, inserted: { ...d.inserted, tts: true } } }, clips: [...s.clips, ...add] };
    });
  }, [update]);

  const runOne = useCallback(
    (vid: string, key: PipeKey, after?: () => void) => {
      patchDub(vid, (d) => ({ pipe: { ...d.pipe, [key]: "running" } }));
      timeouts.current[vid + key] = setTimeout(() => {
        patchDub(vid, (d) => ({ pipe: { ...d.pipe, [key]: "done" } }));
        if (key === "tach") splitAudio(vid);
        if (after) timeouts.current[vid + key + "2"] = setTimeout(after, 280);
      }, 850);
    },
    [patchDub, splitAudio],
  );

  const run = useCallback(
    (key: PipeKey) => {
      const vid = dubVidId();
      if (!vid || getDub(vid).pipe[key] === "running") return;
      runOne(vid, key);
    },
    [getDub, runOne],
  );

  const runAll = useCallback(() => {
    const vid = dubVidId();
    if (!vid) return;
    runOne(vid, "tach", () =>
      runOne(vid, "phan", () => {
        insertSubs(vid, "szh");
        runOne(vid, "dich", () => {
          insertSubs(vid, "svi");
          runOne(vid, "tts", () => insertTts(vid));
        });
      }),
    );
  }, [runOne, insertSubs, insertTts]);

  const addVideo = useCallback(() => {
    update((s) => {
      const n = s.clips.filter((c) => c.type === "video").length + 1;
      const id = "vid" + n;
      const start = Math.max(0, ...s.clips.map((c) => c.start + c.dur));
      const v: Clip = { id, track: "V1", type: "video", name: "clip_" + n + ".mp4", start, dur: 8, scale: 100, posY: 0, opacity: 100, vol: 100, bri: 0, con: 0, sat: 0, thumb: THUMB_VIDEO_ALT };
      const aud: Clip = { id: "aud_" + id, track: "A1", type: "audio", kind: "orig", name: "Âm thanh gốc", srcVideo: id, start, dur: 8, vol: 100, speed: 100, fadeIn: 0, fadeOut: 0 };
      return { sel: id, playhead: start + 0.2, clips: [...s.clips, v, aud], dub: { ...s.dub, [id]: defDub() } };
    });
  }, [update]);

  const addImage = useCallback(() => {
    update((s) => {
      if (s.clips.find((c) => c.id === "img1")) return { sel: "img1" };
      const st0 = Math.max(0, Math.min(totalDur(s.clips) - 3, s.playhead));
      return {
        sel: "img1",
        playhead: st0 + 0.2,
        clips: [...s.clips, { id: "img1", track: "IMG", type: "image", name: "logo.png", start: st0, dur: 3, scale: 100, posY: 0, opacity: 100, ox: 60, oy: 9, thumb: THUMB_IMAGE }],
      };
    });
  }, [update]);

  const addMusic = useCallback(() => {
    update((s) =>
      s.clips.find((c) => c.id === "bgm")
        ? {}
        : { sel: "bgm", clips: [...s.clips, { id: "bgm", track: "A2", type: "audio", kind: "music", name: "nhac_nen.mp3", start: 0, dur: 11, vol: 50, speed: 100, fadeIn: 0, fadeOut: 0 }] },
    );
  }, [update]);

  // ── edit selected ──
  const setClipNum = useCallback((key: keyof Clip, value: number) => {
    const id = stateRef.current.sel;
    update((s) => ({ clips: s.clips.map((c) => (c.id === id ? { ...c, [key]: value } : c)) }));
  }, [update]);

  const setClipText = useCallback((value: string) => {
    const id = stateRef.current.sel;
    update((s) => ({ clips: s.clips.map((c) => (c.id === id ? { ...c, text: value, name: value } : c)) }));
  }, [update]);

  const resetColor = useCallback(() => {
    const id = stateRef.current.sel;
    update((s) => ({ clips: s.clips.map((c) => (c.id === id ? { ...c, bri: 0, con: 0, sat: 0 } : c)) }));
  }, [update]);

  const setSubNum = useCallback((key: "size" | "pos", value: number) => update((s) => ({ subStyle: { ...s.subStyle, [key]: value } })), [update]);
  const setSubColor = useCallback((color: string) => update((s) => ({ subStyle: { ...s.subStyle, color } })), [update]);
  const toggleSubBg = useCallback(() => update((s) => ({ subStyle: { ...s.subStyle, bg: !s.subStyle.bg } })), [update]);
  const toggleBil = useCallback(() => update((s) => ({ subStyle: { ...s.subStyle, bilingual: !s.subStyle.bilingual } })), [update]);
  const seedSubStyle = useCallback((partial: Partial<StudioState["subStyle"]>) => update((s) => ({ subStyle: { ...s.subStyle, ...partial } })), [update]);

  const setVoice = useCallback((v: string) => {
    const vid = dubVidId();
    if (vid) patchDub(vid, () => ({ voice: v }));
  }, [patchDub]);

  const toggleSnap = useCallback(() => update((s) => ({ snap: !s.snap })), [update]);
  const setAspect = useCallback((a: StudioState["aspect"]) => update(() => ({ aspect: a })), [update]);
  const zoomIn = useCallback(() => update((s) => ({ zoom: Math.min(400, s.zoom + 25) })), [update]);
  const zoomOut = useCallback(() => update((s) => ({ zoom: Math.max(25, s.zoom - 25) })), [update]);

  const splitSel = useCallback(() => {
    const { sel: id, playhead: ph } = stateRef.current;
    update((s) => {
      const c = s.clips.find((x) => x.id === id);
      if (!c || ph <= c.start + 0.2 || ph >= c.start + c.dur - 0.2) return {};
      const a = { ...c, dur: ph - c.start };
      const b = { ...c, id: c.id + "_b" + Math.floor(Math.random() * 999), start: ph, dur: c.start + c.dur - ph };
      return { clips: s.clips.flatMap((x) => (x.id === id ? [a, b] : [x])) };
    });
  }, [update]);

  const delSel = useCallback(() => {
    const id = stateRef.current.sel;
    if (id === "vid") return;
    update((s) => ({ clips: s.clips.filter((c) => c.id !== id), sel: null }));
  }, [update]);

  const setTab = useCallback((t: "media" | "dub") => update(() => ({ tab: t })), [update]);

  const replaceClips = useCallback(
    (clips: Clip[]) =>
      update((s) => ({ clips, sel: clips.some((c) => c.id === s.sel) ? s.sel : (clips[0]?.id ?? null) })),
    [update],
  );

  const actions: StudioActions = {
    setTab, onMove, onUp, imgDown, clipDown, rulerDown, deselect, selectTrack, togglePlay, toStart,
    setPrev: (el) => { prevRef.current = el; },
    setLane: (el) => { laneRef.current = el; },
    getDub, patchDub, run, runAll, insertSubs, insertTts, addVideo, addImage, addMusic,
    setClipNum, setClipText, resetColor, setSubNum, setSubColor, seedSubStyle, toggleSubBg, toggleBil,
    setVoice, toggleSnap, setAspect, zoomIn, zoomOut, splitSel, delSel, replaceClips,
  };

  return { state, actions, refs: { prev: prevRef, lane: laneRef } };
}
