import { useEffect, useRef, useState } from "react";
import { C, FONT } from "../theme";
import { useStudio } from "./useStudio";
import { useDubProject } from "./useDubProject";
import { useTransport } from "./useTransport";
import { buildClips, clipSignature } from "./seed";
import { trackAudioKind, type TrackCtl } from "./trackmap";
import { TopBar } from "./TopBar";
import { LeftPanel } from "./LeftPanel";
import { PreviewStage } from "./PreviewStage";
import { Inspector } from "./Inspector";
import { Timeline } from "./Timeline";

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
  const [title, setTitle] = useState(initialTitle ?? "Dự án lồng tiếng");

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
      const k = trackAudioKind(key);
      return k === "original" ? p.original_volume > 0 : k === "vn" ? p.vn_volume > 0 : k === "sub" ? p.burn_subtitles : true;
    },
    toggle: (key) => {
      const p = dub.detail?.project;
      if (!p) return;
      const k = trackAudioKind(key);
      if (k === "original") void dub.patchSettings({ original_volume: p.original_volume > 0 ? 0 : lastVol.current.original });
      else if (k === "vn") void dub.patchSettings({ vn_volume: p.vn_volume > 0 ? 0 : lastVol.current.vn });
      else if (k === "sub") void dub.patchSettings({ burn_subtitles: !p.burn_subtitles });
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
      <TopBar title={title} onTitle={setTitle} onTitleCommit={(v) => void dub.rename(v)} snap={state.snap} onToggleSnap={actions.toggleSnap} dub={dub} />
      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        <LeftPanel state={state} actions={actions} dub={dub} />
        <PreviewStage state={state} actions={actions} dub={dub} transport={transport} />
        <Inspector state={state} actions={actions} dub={dub} transport={transport} trackCtl={trackCtl} />
      </div>
      <Timeline state={state} actions={actions} transport={transport} trackCtl={trackCtl} />
    </div>
  );
}
