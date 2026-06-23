import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getVoices, type Voice } from "../../api";
import {
  cancelDub,
  composeClips,
  createClip,
  createOverlay,
  deleteClip,
  deleteOverlay,
  dubVideoUrl,
  fileUrl,
  getDubInfo,
  getDubProject,
  listClones,
  runDubStep,
  setDubSegmentOffset,
  setDubSpeakerVoice,
  setDubVideoOffset,
  updateClip,
  updateDubSegment,
  updateDubSettings,
  updateOverlay,
  type DubClipGeo,
  type DubDetail,
  type DubMediaInfo,
  type DubOverlay,
  type DubOverlayGeo,
  type DubSettings,
  type DubStep,
  type VoiceClone,
} from "../../studioApi";
import { settingsOf, type VoiceOpt } from "../../components/dubbing/shared";

/** A status is "busy" while a step is running (…ing), e.g. extracting / synthesizing. */
export const isBusy = (s: string) => s.endsWith("ing") && s !== "pending";

export interface PipeStep {
  step: DubStep;
  from: string;
  busy: string;
  done: string;
}
export const STEPS: PipeStep[] = [
  { step: "extract", from: "created", busy: "extracting", done: "extracted" },
  { step: "analyze", from: "extracted", busy: "analyzing", done: "analyzed" },
  { step: "translate", from: "analyzed", busy: "translating", done: "translated" },
  { step: "synthesize", from: "translated", busy: "synthesizing", done: "synthesized" },
  { step: "build", from: "synthesized", busy: "building", done: "built" },
  { step: "export", from: "built", busy: "exporting", done: "done" },
];
export const ORDER = ["created", "extracted", "analyzed", "translated", "synthesized", "built", "done"];

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export interface DubProjectHook {
  detail: DubDetail | null;
  info: DubMediaInfo | null;
  voices: Voice[];
  voiceOpts: VoiceOpt[];
  genderBySpeaker: Record<string, string | null>;
  longCount: number;
  err: string | null;
  busy: boolean;
  autoRun: boolean;
  videoUrl: string;
  vnUrl: string;
  reachedIdx: number;
  /** seconds, from probed media or furthest segment */
  duration: number;
  refresh: () => Promise<void>;
  /** Run a step; `force` (synthesize only) regenerates, bypassing the TTS cache. */
  run: (step: DubStep, force?: boolean) => Promise<void>;
  runTo: (target: "synthesized" | "done") => Promise<void>;
  cancel: () => Promise<void>;
  rename: (name: string) => Promise<void>;
  /** Merge a partial settings patch (volume, burn, …) and persist it. */
  patchSettings: (partial: Partial<DubSettings>) => Promise<void>;
  setSegment: (segId: string, textVi: string, voice: string | null) => Promise<void>;
  setAllSpeakerVoice: (voice: string | null) => Promise<void>;
  reshorten: () => Promise<void>;
  /** Image/banner overlays. */
  overlays: DubOverlay[];
  addOverlay: (file: Blob, geo?: Partial<DubOverlayGeo>) => Promise<void>;
  patchOverlay: (oid: string, geo: DubOverlayGeo) => Promise<void>;
  removeOverlay: (oid: string) => Promise<void>;
  /** Shift a dubbed line on the timeline (seconds); affects build + export. */
  setSegmentOffset: (segId: string, offsetS: number) => Promise<void>;
  /** Video lead-in (seconds of empty space before the video). */
  setVideoOffset: (offsetS: number) => Promise<void>;
  /** Clip-based timeline (dub_clips). */
  clips: DubDetail["clips"];
  /** Update a user clip's geometry/timing. */
  patchClip: (cid: string, geo: DubClipGeo) => Promise<void>;
  /** Add a media clip from an uploaded file (kind inferred by the caller). */
  addClip: (fields: Record<string, string | number>, file?: Blob) => Promise<void>;
  /** Remove a clip. */
  removeClip: (cid: string) => Promise<void>;
}

/** Real video-dubbing project state: loads + polls a project, exposes the pipeline. */
export function useDubProject(id: string): DubProjectHook {
  const [detail, setDetail] = useState<DubDetail | null>(null);
  const [info, setInfo] = useState<DubMediaInfo | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
  const [clones, setClones] = useState<VoiceClone[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [autoRun, setAutoRun] = useState(false);
  const [videoUrl, setVideoUrl] = useState("");
  const [vnUrl, setVnUrl] = useState("");
  const [mediaVer, setMediaVer] = useState(0);
  const prevStatus = useRef("");

  const refresh = useCallback(async () => {
    try {
      setDetail(await getDubProject(id));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, [id]);

  useEffect(() => {
    void refresh();
    getVoices().then(setVoices).catch(() => {});
    listClones().then(setClones).catch(() => {});
    getDubInfo(id).then(setInfo).catch(() => {});
    void dubVideoUrl(id).then(setVideoUrl);
  }, [id, refresh]);

  const status = detail?.project.status ?? "";
  const busy = isBusy(status);

  // poll while a step is running
  useEffect(() => {
    if (!busy) return;
    const h = setInterval(refresh, 1500);
    return () => clearInterval(h);
  }, [busy, refresh]);

  // Console trace of the pipeline: log every status change and every progress
  // beat (label / %), so a stuck or slow run is visible in the devtools console.
  const prevTrace = useRef("");
  useEffect(() => {
    const p = detail?.project;
    if (!p) return;
    const pct = p.progress == null ? "" : ` ${Math.round(p.progress * 100)}%`;
    const line = `[dub] ${p.status}${pct}${p.progress_label ? " — " + p.progress_label : ""}`;
    if (line !== prevTrace.current) {
      prevTrace.current = line;
      // eslint-disable-next-line no-console
      console.log(line);
    }
  }, [detail?.project.status, detail?.project.progress, detail?.project.progress_label]);

  // bump media version only when the VN track was (re)built, so settings edits
  // don't reload the player to a black frame.
  useEffect(() => {
    const s = detail?.project.status;
    if (!s) return;
    const prev = prevStatus.current;
    if ((prev === "synthesizing" || prev === "building" || prev === "exporting") && s !== prev) {
      setMediaVer((v) => v + 1);
    }
    prevStatus.current = s;
  }, [detail?.project.status]);

  const vnPath = detail?.project.vn_track_path ?? null;
  useEffect(() => {
    if (vnPath) void fileUrl(vnPath).then((u) => setVnUrl(`${u}&v=${mediaVer}`));
    else setVnUrl("");
  }, [vnPath, mediaVer]);

  const reachedIdx = ORDER.indexOf(status === "failed" || busy ? "" : status);

  const run = useCallback(
    async (step: DubStep, force?: boolean) => {
      setErr(null);
      try {
        await runDubStep(id, step, force);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [id, refresh],
  );

  const runTo = useCallback(
    async (target: "synthesized" | "done") => {
      setErr(null);
      setAutoRun(true);
      try {
        let cur = await getDubProject(id);
        setDetail(cur);
        for (const s of STEPS) {
          if (ORDER.indexOf(s.done) > ORDER.indexOf(target)) break;
          if (ORDER.indexOf(s.done) <= ORDER.indexOf(cur.project.status)) continue;
          await runDubStep(id, s.step);
          for (;;) {
            await sleep(1500);
            cur = await getDubProject(id);
            setDetail(cur);
            if (cur.project.status === s.done) break;
            // On failure the backend reverts status to the step's `from` (not the
            // literal "failed") and sets an error — detect that so auto-run stops
            // instead of polling forever.
            const errMsg = cur.project.error?.trim();
            if (errMsg && !isBusy(cur.project.status)) throw new Error(errMsg);
          }
        }
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      } finally {
        setAutoRun(false);
        await refresh();
      }
    },
    [id, refresh],
  );

  const cancel = useCallback(async () => {
    await cancelDub(id);
    await refresh();
  }, [id, refresh]);

  const rename = useCallback(
    async (name: string) => {
      if (!detail) return;
      await updateDubSettings(id, settingsOf(detail.project, { name }));
      await refresh();
    },
    [id, detail, refresh],
  );

  const patchSettings = useCallback(
    async (partial: Partial<DubSettings>) => {
      if (!detail) return;
      await updateDubSettings(id, settingsOf(detail.project, partial));
      await refresh();
    },
    [id, detail, refresh],
  );

  const setSegment = useCallback(
    async (segId: string, textVi: string, voice: string | null) => {
      await updateDubSegment(segId, textVi, voice);
      await refresh();
    },
    [refresh],
  );

  // After "Đọc TTS" (synthesize), the per-segment audio exists but the merged VN
  // track (needed to hear the dub in preview) is only produced by build. Build it
  // automatically, once, so the preview gets sound without waiting for export.
  const autoBuilt = useRef("");
  useEffect(() => {
    const p = detail?.project;
    if (!p) return;
    if (p.status === "synthesized" && !p.vn_track_path && !busy && !autoRun && autoBuilt.current !== id) {
      autoBuilt.current = id;
      void run("build");
    }
  }, [detail?.project.status, detail?.project.vn_track_path, busy, autoRun, id, run]);

  const setAllSpeakerVoice = useCallback(
    async (voice: string | null) => {
      if (!detail) return;
      await Promise.all(detail.speakers.map((sp) => setDubSpeakerVoice(id, sp.speaker, voice)));
      await refresh();
    },
    [id, detail, refresh],
  );

  const reshorten = useCallback(async () => {
    setErr(null);
    try {
      await runDubStep(id, "reshorten");
      await refresh();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, [id, refresh]);

  const addOverlay = useCallback(
    async (file: Blob, geo?: Partial<DubOverlayGeo>) => {
      setErr(null);
      try {
        await createOverlay(id, file, geo);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [id, refresh],
  );
  const patchOverlay = useCallback(
    async (oid: string, geo: DubOverlayGeo) => {
      try {
        await updateOverlay(oid, geo);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [refresh],
  );
  const removeOverlay = useCallback(
    async (oid: string) => {
      try {
        await deleteOverlay(oid);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [refresh],
  );
  const setSegmentOffset = useCallback(
    async (segId: string, offsetS: number) => {
      try {
        await setDubSegmentOffset(segId, offsetS);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [refresh],
  );
  const setVideoOffset = useCallback(
    async (offsetS: number) => {
      try {
        await setDubVideoOffset(id, Math.max(0, offsetS));
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [id, refresh],
  );
  const patchClip = useCallback(
    async (cid: string, geo: DubClipGeo) => {
      try {
        await updateClip(cid, geo);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [refresh],
  );
  const addClip = useCallback(
    async (fields: Record<string, string | number>, file?: Blob) => {
      try {
        await createClip(id, fields, file);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [id, refresh],
  );
  const removeClip = useCallback(
    async (cid: string) => {
      try {
        await deleteClip(cid);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [refresh],
  );

  // Back-fill the clip model on first open: if the project has analysed content
  // but no clips yet, generate them from the dub data once.
  const composed = useRef("");
  useEffect(() => {
    const d = detail;
    if (!d) return;
    if (d.clips.length > 0) return;
    const hasContent = d.segments.length > 0 || !!d.project.video_path;
    if (!hasContent || composed.current === id) return;
    composed.current = id;
    composeClips(id)
      .then(() => refresh())
      .catch(() => {});
  }, [detail, id, refresh]);

  const segments = detail?.segments ?? [];
  // Engine presets + on-disk clones (bundled voice pack + user clones). Clones
  // are stored as `clone:<id>` and resolved to a ref_id at synth time.
  const voiceOpts: VoiceOpt[] = useMemo(
    () => [
      ...voices.map((v) => ({ value: v.id, label: v.label })),
      ...clones.map((c) => ({
        value: `clone:${c.id}`,
        label: c.builtin ? `${c.name} (bộ giọng)` : `${c.name} (của bạn)`,
      })),
    ],
    [voices, clones],
  );
  const genderBySpeaker = useMemo(
    () => Object.fromEntries((detail?.speakers ?? []).map((sp) => [sp.speaker, sp.gender])),
    [detail?.speakers],
  );
  const longCount = segments.filter((s) => s.status === "long").length;
  const duration = info?.duration ?? (segments.length ? Math.max(...segments.map((s) => s.end_s)) : 0);

  return {
    detail,
    info,
    voices,
    voiceOpts,
    genderBySpeaker,
    longCount,
    err,
    busy,
    autoRun,
    videoUrl,
    vnUrl,
    reachedIdx,
    duration,
    refresh,
    run,
    runTo,
    cancel,
    rename,
    patchSettings,
    setSegment,
    setAllSpeakerVoice,
    reshorten,
    overlays: detail?.overlays ?? [],
    addOverlay,
    patchOverlay,
    removeOverlay,
    setSegmentOffset,
    setVideoOffset,
    clips: detail?.clips ?? [],
    patchClip,
    addClip,
    removeClip,
  };
}
