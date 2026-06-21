import { useEffect, useState } from "react";
import { C, FONT } from "../theme";
import { useStudio } from "./useStudio";
import { useDubProject } from "./useDubProject";
import { useTransport } from "./useTransport";
import { buildClips, clipSignature } from "./seed";
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

  return (
    <div style={{ height: "100%", width: "100%", display: "flex", flexDirection: "column", background: C.appBg, color: "#fff", fontFamily: FONT, fontSize: 13, overflow: "hidden", userSelect: "none", WebkitFontSmoothing: "antialiased" }}>
      <TopBar title={title} onTitle={setTitle} onTitleCommit={(v) => void dub.rename(v)} snap={state.snap} onToggleSnap={actions.toggleSnap} dub={dub} />
      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        <LeftPanel state={state} actions={actions} dub={dub} />
        <PreviewStage state={state} actions={actions} dub={dub} transport={transport} />
        <Inspector state={state} actions={actions} dub={dub} />
      </div>
      <Timeline state={state} actions={actions} transport={transport} />
    </div>
  );
}
