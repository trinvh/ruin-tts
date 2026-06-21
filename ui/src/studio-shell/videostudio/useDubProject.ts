import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getVoices, type Voice } from "../../api";
import {
  cancelDub,
  dubVideoUrl,
  fileUrl,
  getDubInfo,
  getDubProject,
  runDubStep,
  setDubSpeakerVoice,
  updateDubSegment,
  updateDubSettings,
  type DubDetail,
  type DubMediaInfo,
  type DubSettings,
  type DubStep,
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
  run: (step: DubStep) => Promise<void>;
  runTo: (target: "synthesized" | "done") => Promise<void>;
  cancel: () => Promise<void>;
  rename: (name: string) => Promise<void>;
  /** Merge a partial settings patch (volume, burn, …) and persist it. */
  patchSettings: (partial: Partial<DubSettings>) => Promise<void>;
  setSegment: (segId: string, textVi: string, voice: string | null) => Promise<void>;
  setAllSpeakerVoice: (voice: string | null) => Promise<void>;
  reshorten: () => Promise<void>;
}

/** Real video-dubbing project state: loads + polls a project, exposes the pipeline. */
export function useDubProject(id: string): DubProjectHook {
  const [detail, setDetail] = useState<DubDetail | null>(null);
  const [info, setInfo] = useState<DubMediaInfo | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
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
    async (step: DubStep) => {
      setErr(null);
      try {
        await runDubStep(id, step);
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
            if (cur.project.status === "failed") throw new Error(cur.project.error ?? "lỗi không rõ");
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

  const segments = detail?.segments ?? [];
  const voiceOpts: VoiceOpt[] = useMemo(() => voices.map((v) => ({ value: v.id, label: v.label })), [voices]);
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
  };
}
