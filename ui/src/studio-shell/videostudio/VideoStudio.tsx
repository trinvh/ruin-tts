import { useEffect, useRef, useState } from "react";
import { C, FONT } from "../theme";
import { useStudio } from "./useStudio";
import { useDubProject } from "./useDubProject";
import { useTransport } from "./useTransport";
import { useEditorHistory } from "./useEditorHistory";
import { buildClips, clipSignature } from "./seed";
import { trackAudioKind, type TrackCtl } from "./trackmap";
import { TopBar } from "./TopBar";
import { LeftPanel } from "./LeftPanel";
import { PreviewStage } from "./PreviewStage";
import { Inspector } from "./Inspector";
import { Timeline } from "./Timeline";
import { HistoryPanel } from "./HistoryPanel";

interface Props {
  /** Real dub project id — the editor loads + drives this project. */
  projectId: string;
  /** Initial tab title (project name); refined once the project loads. */
  title?: string;
}

/**
 * Video Studio — the per-project editor, wired to the real dubbing backend.
 * The Beesoft chrome (timeline, inspector, transport) is preserved, but the dub
 * pipeline, preview playback, transcript and export all hit the studio server.
 */
export function VideoStudio({ projectId, title: initialTitle }: Props) {
  const dub = useDubProject(projectId);
  const { state, actions } = useStudio();
  const transport = useTransport();
  const history = useEditorHistory(dub);
  const [title, setTitle] = useState(initialTitle ?? "Dự án lồng tiếng");
  const [historyOpen, setHistoryOpen] = useState(false);

  // Undo/redo keyboard shortcuts (preview-effective; persisted so export matches).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "z") return;
      const el = document.activeElement;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA")) return;
      e.preventDefault();
      if (e.shiftKey) history.redo();
      else history.undo();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [history]);

  const projectName = dub.detail?.project.name;
  useEffect(() => {
    if (projectName) setTitle(projectName);
  }, [projectName]);

  // Seed the timeline visualisation from real project data whenever it changes.
  const sig = clipSignature(dub.detail, dub.duration);
  useEffect(() => {
    actions.replaceClips(buildClips(dub.detail, dub.duration));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sig]);

  // Remember the last non-zero volumes so the eye toggle can restore them.
  const lastVol = useRef({ original: 0.15, vn: 1 });
  const proj = dub.detail?.project;
  useEffect(() => {
    if (!proj) return;
    if (proj.original_volume > 0) lastVol.current.original = proj.original_volume;
    if (proj.vn_volume > 0) lastVol.current.vn = proj.vn_volume;
  }, [proj?.original_volume, proj?.vn_volume]);

  // Seed the live-preview subtitle style from the persisted project so the
  // editor opens reflecting saved size/colour/bilingual values.
  useEffect(() => {
    if (!proj) return;
    actions.seedSubStyle({ size: proj.sub_size, color: proj.sub_color, bilingual: proj.sub_bilingual });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [proj?.sub_size, proj?.sub_color, proj?.sub_bilingual]);

  const trackCtl: TrackCtl = {
    kindOf: trackAudioKind,
    hasEye: (key) => trackAudioKind(key) !== null,
    enabled: (key) => {
      const p = dub.detail?.project;
      if (!p) return true;
      switch (trackAudioKind(key)) {
        case "video": return p.video_enabled;
        case "original": return p.original_volume > 0;
        case "vn": return p.vn_volume > 0;
        case "subSrc": return p.sub_bilingual;
        case "sub": return p.burn_subtitles;
        default: return true;
      }
    },
    // Toggling a track is an undoable edit → goes through the history.
    toggle: (key) => {
      const p = dub.detail?.project;
      if (!p) return;
      switch (trackAudioKind(key)) {
        case "video":
          history.commit(p.video_enabled ? "Xoá track Video" : "Khôi phục track Video", { video_enabled: !p.video_enabled });
          break;
        case "original": {
          const on = p.original_volume > 0;
          history.commit(on ? "Xoá track Tiếng gốc" : "Khôi phục track Tiếng gốc", { original_volume: on ? 0 : lastVol.current.original });
          break;
        }
        case "vn": {
          const on = p.vn_volume > 0;
          history.commit(on ? "Xoá track Lồng tiếng" : "Khôi phục track Lồng tiếng", { vn_volume: on ? 0 : lastVol.current.vn });
          break;
        }
        case "subSrc":
          history.commit(p.sub_bilingual ? "Xoá phụ đề gốc" : "Khôi phục phụ đề gốc", { sub_bilingual: !p.sub_bilingual });
          break;
        case "sub":
          history.commit(p.burn_subtitles ? "Xoá phụ đề tiếng Việt" : "Khôi phục phụ đề tiếng Việt", { burn_subtitles: !p.burn_subtitles });
          break;
      }
    },
    volume: (key) => {
      const p = dub.detail?.project;
      if (!p) return null;
      const k = trackAudioKind(key);
      return k === "original" ? p.original_volume : k === "vn" ? p.vn_volume : null;
    },
    setVolume: (key, v) => {
      const k = trackAudioKind(key);
      if (k === "original") void dub.patchSettings({ original_volume: v });
      else if (k === "vn") void dub.patchSettings({ vn_volume: v });
    },
  };

  return (
    <div style={{ height: "100%", width: "100%", display: "flex", flexDirection: "column", background: C.appBg, color: "#fff", fontFamily: FONT, fontSize: 13, overflow: "hidden", userSelect: "none", WebkitFontSmoothing: "antialiased" }}>
      <TopBar title={title} onTitle={setTitle} onTitleCommit={(v) => void dub.rename(v)} snap={state.snap} onToggleSnap={actions.toggleSnap} dub={dub} history={history} historyOpen={historyOpen} onToggleHistory={() => setHistoryOpen((v) => !v)} />
      <div style={{ flex: 1, display: "flex", minHeight: 0, position: "relative" }}>
        <LeftPanel state={state} actions={actions} dub={dub} />
        <PreviewStage state={state} actions={actions} dub={dub} transport={transport} />
        <Inspector state={state} actions={actions} dub={dub} transport={transport} trackCtl={trackCtl} />
        {historyOpen && <HistoryPanel history={history} onClose={() => setHistoryOpen(false)} />}
      </div>
      <Timeline state={state} actions={actions} transport={transport} trackCtl={trackCtl} />
    </div>
  );
}
