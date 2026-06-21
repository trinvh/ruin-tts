import { useState } from "react";
import { C, FONT } from "../theme";
import { useStudio } from "./useStudio";
import { TopBar } from "./TopBar";
import { LeftPanel } from "./LeftPanel";
import { PreviewStage } from "./PreviewStage";
import { Inspector } from "./Inspector";
import { Timeline } from "./Timeline";

interface Props {
  /** Project title shown (and editable) in the editor top bar. */
  title?: string;
}

/**
 * Video Studio — the per-project CapCut-style editor. A faithful front-end-only
 * visual port of the design's "Video Studio.dc.html": timeline drag/trim/zoom,
 * inspector sliders, and a mocked dub pipeline all run client-side.
 */
export function VideoStudio({ title: initialTitle }: Props) {
  const { state, actions } = useStudio();
  const [title, setTitle] = useState(initialTitle ?? "Dự án lồng tiếng");

  return (
    <div style={{ height: "100%", width: "100%", display: "flex", flexDirection: "column", background: C.appBg, color: "#fff", fontFamily: FONT, fontSize: 13, overflow: "hidden", userSelect: "none", WebkitFontSmoothing: "antialiased" }}>
      <TopBar title={title} onTitle={setTitle} state={state} actions={actions} />
      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        <LeftPanel state={state} actions={actions} />
        <PreviewStage state={state} actions={actions} />
        <Inspector state={state} actions={actions} />
      </div>
      <Timeline state={state} actions={actions} />
    </div>
  );
}
