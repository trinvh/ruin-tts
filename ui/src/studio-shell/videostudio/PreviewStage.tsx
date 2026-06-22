import { useEffect, useRef, useState } from "react";
import { C, MONO } from "../theme";
import { Icon } from "../icons";
import { HoverBox } from "../ui";
import { fmt } from "./constants";
import { OverlayLayer } from "./OverlayLayer";
import { ClipPreview } from "./ClipPreview";
import { DubAudioLayer } from "./DubAudioLayer";
import { SUB_FONT, SUB_OUTLINE, subBoxStyle, subFontSize } from "./subtitleStyle";
import type { StudioActions, StudioState } from "./useStudio";
import type { DubProjectHook } from "./useDubProject";
import type { Transport } from "./useTransport";
import type { Aspect } from "./types";

const clamp01 = (v: number) => Math.max(0, Math.min(1, v));

interface Props {
  state: StudioState;
  actions: StudioActions;
  dub: DubProjectHook;
  transport: Transport;
}

const ctlBtn: React.CSSProperties = { width: 32, height: 32, border: "none", background: "transparent", color: C.steel, borderRadius: 7, display: "grid", placeItems: "center", cursor: "pointer" };
const ctlHover: React.CSSProperties = { background: C.panel3, color: "#fff" };
const ASPECTS: Aspect[] = ["9:16", "1:1", "16:9"];

export function PreviewStage({ state, actions, dub, transport }: Props) {
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [dropActive, setDropActive] = useState(false);

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDropActive(false);
    const file = Array.from(e.dataTransfer.files).find((f) => f.type.startsWith("image/"));
    if (!file) return;
    const box = boxRef.current?.getBoundingClientRect();
    const x = box ? clamp01((e.clientX - box.left) / box.width - 0.15) : 0.05;
    const y = box ? clamp01((e.clientY - box.top) / box.height - 0.1) : 0.05;
    void dub.addOverlay(file, { x, y, w: 0.3, opacity: 1, start_s: 0, end_s: 0 });
  };
  const { subStyle, aspect } = state;
  const t = transport.time;
  const videoOffset = dub.detail?.project.video_offset_s ?? 0;
  const videoDur = dub.duration;
  // Active dub subtitle taken from the CLIPS (so timeline drags/positions show).
  const subClip = (dub.clips ?? []).find(
    (c) => c.origin?.startsWith("dub:sub") && t >= c.start_s && t < c.start_s + c.dur_s,
  );
  const capVi = subClip?.text?.trim() ?? "";
  const capZh = (() => {
    if (!subClip || !subStyle.bilingual) return "";
    const segId = subClip.origin.slice("dub:sub:".length);
    return dub.detail?.segments.find((s) => s.id === segId)?.text_src?.trim() ?? "";
  })();
  // Timeline length = furthest clip end (covers lead-in + added media).
  const clipsEnd = (dub.clips ?? []).reduce((m, c) => Math.max(m, c.start_s + c.dur_s), 0);
  const total = Math.max(clipsEnd, videoOffset + videoDur) || videoDur;
  // The Vietnamese subtitle track (eye = burn_subtitles) gates preview + export.
  const subsOn = dub.detail?.project.burn_subtitles ?? false;
  // Video track deleted → audio-only placeholder; otherwise frames show only
  // while the playhead is within the (offset-delayed) video span.
  const videoEnabled = dub.detail?.project.video_enabled ?? true;
  const videoVisible =
    videoEnabled && t >= videoOffset - 0.05 && (videoDur <= 0 || t <= videoOffset + videoDur + 0.05);
  const showCaption = videoEnabled && subsOn && !!(capVi || capZh);
  const showCapZh = !!(subStyle.bilingual && capZh && capVi);
  const aspectCss = aspect === "9:16" ? "9 / 16" : aspect === "1:1" ? "1 / 1" : "16 / 9";
  const origVol = dub.detail?.project.original_volume ?? 1;
  const vnVol = dub.detail?.project.vn_volume ?? 1;

  // Feed the independent clock its lead-in + total length.
  useEffect(() => {
    transport.setVideoOffset(videoOffset);
    transport.setDuration(total);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [videoOffset, total]);

  const onLoaded = () => {
    transport.onLoaded();
    transport.setOrigVolume(origVol);
    transport.setVnVolume(vnVol);
  };

  // Keep live playback volumes in sync with the persisted track settings. Keyed
  // on the value only (not `transport`, whose methods read live refs) so a
  // playing video's re-renders don't stomp a live inspector drag.
  useEffect(() => {
    transport.setOrigVolume(origVol);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [origVol]);
  useEffect(() => {
    transport.setVnVolume(vnVol);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vnVol]);

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0, background: C.previewBg }}>
      <div
        style={{
          flex: 1, position: "relative", display: "grid", placeItems: "center", overflow: "hidden", padding: 20,
          backgroundColor: C.checker,
          backgroundImage: `linear-gradient(45deg,${C.checkerAlt} 25%,transparent 25%),linear-gradient(-45deg,${C.checkerAlt} 25%,transparent 25%),linear-gradient(45deg,transparent 75%,${C.checkerAlt} 75%),linear-gradient(-45deg,transparent 75%,${C.checkerAlt} 75%)`,
          backgroundSize: "22px 22px",
          backgroundPosition: "0 0,0 11px,11px -11px,-11px 0",
        }}
      >
        <div
          ref={(el) => { actions.setPrev(el); boxRef.current = el; }}
          onDragOver={(e) => { e.preventDefault(); setDropActive(true); }}
          onDragLeave={() => setDropActive(false)}
          onDrop={onDrop}
          style={{ position: "relative", height: "100%", aspectRatio: aspectCss, maxWidth: "100%", borderRadius: 6, overflow: "hidden", background: "#000", containerType: "size", boxShadow: dropActive ? `0 0 0 2px ${C.coral}` : "0 10px 40px rgba(0,0,0,.55),0 0 0 1px rgba(255,255,255,.05)" }}
        >
          <video
            ref={(el) => transport.attachVideo(el)}
            src={dub.videoUrl || undefined}
            playsInline
            style={{ width: "100%", height: "100%", objectFit: "contain", display: "block", background: "#000", visibility: videoVisible ? "visible" : "hidden" }}
            onLoadedMetadata={onLoaded}
          />
          {!videoEnabled && (
            <div style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center", background: "#000", color: C.muted2, pointerEvents: "none" }}>
              <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
                <Icon name="wave" size={34} stroke={1.6} color={C.muted3} />
                <span style={{ fontSize: 12.5 }}>Chỉ âm thanh (đã xoá track video)</span>
              </div>
            </div>
          )}
          {showCaption && (
            <div style={subBoxStyle()}>
              {showCapZh && (
                <div style={{ fontFamily: SUB_FONT, fontWeight: 600, color: "#fff", opacity: 0.9, textShadow: SUB_OUTLINE, fontSize: subFontSize(subStyle.size * 0.72), marginBottom: "calc(.4 * 1cqh)" }}>{capZh}</div>
              )}
              <span style={{ fontFamily: SUB_FONT, fontWeight: 600, color: subStyle.color, textShadow: SUB_OUTLINE, fontSize: subFontSize(subStyle.size), lineHeight: 1.3, background: subStyle.bg ? "rgba(0,0,0,.5)" : "transparent", padding: subStyle.bg ? "0 .35em" : undefined, WebkitBoxDecorationBreak: "clone", boxDecorationBreak: "clone" }}>{capVi || capZh}</span>
            </div>
          )}
          <OverlayLayer overlays={dub.overlays} time={t} stageRef={boxRef} onUpdate={(oid, geo) => void dub.patchOverlay(oid, geo)} onDelete={(oid) => void dub.removeOverlay(oid)} />
          <ClipPreview clips={dub.clips} time={t} stageRef={boxRef} playing={transport.playing} onUpdate={(cid, geo) => void dub.patchClip(cid, geo)} />
          {dropActive && (
            <div style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center", background: "rgba(234,124,105,.18)", color: "#fff", pointerEvents: "none", fontSize: 13, fontWeight: 600 }}>
              Thả ảnh banner vào đây
            </div>
          )}
        </div>
        {/* Vietnamese dub played per segment from the clips, so timeline drags
            move the voice live (not the pre-merged track). */}
        <DubAudioLayer clips={dub.clips} time={t} playing={transport.playing} volume={vnVol} />
      </div>

      {/* transport */}
      <div style={{ flex: "none", height: 52, display: "flex", alignItems: "center", padding: "0 16px", borderTop: `1px solid ${C.border}`, background: C.panel }}>
        <div style={{ flex: 1, display: "flex", alignItems: "center", gap: 8, fontFamily: MONO, fontSize: 13 }}>
          <span style={{ color: "#fff", fontWeight: 500 }}>{fmt(t)}</span>
          <span style={{ color: C.muted5 }}>/</span>
          <span style={{ color: C.muted2 }}>{fmt(total)}</span>
        </div>
        <div style={{ flex: "none", display: "flex", alignItems: "center", gap: 6 }}>
          <HoverBox as="button" onClick={transport.toStart} style={ctlBtn} hoverStyle={ctlHover}><Icon name="toStart" size={18} /></HoverBox>
          <HoverBox
            as="button"
            onClick={transport.togglePlay}
            style={{ width: 40, height: 40, border: "none", background: C.coral, color: "#fff", borderRadius: "50%", display: "grid", placeItems: "center", cursor: "pointer", boxShadow: "0 4px 14px rgba(234,124,105,.45)" }}
            hoverStyle={{ background: C.coralLt }}
            activeStyle={{ transform: "scale(.95)" }}
          >
            {transport.playing ? <Icon name="pause" size={17} /> : <Icon name="play" size={18} style={{ marginLeft: 2 }} />}
          </HoverBox>
          <HoverBox as="button" onClick={() => transport.seek(total)} style={ctlBtn} hoverStyle={ctlHover}><Icon name="toEnd" size={18} /></HoverBox>
        </div>
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 10 }}>
          <div style={{ display: "flex", background: C.panel2, border: `1px solid ${C.border}`, borderRadius: 8, padding: 2, gap: 2 }}>
            {ASPECTS.map((a) => (
              <button key={a} onClick={() => actions.setAspect(a)} style={{ border: "none", background: a === aspect ? C.coral : "transparent", color: a === aspect ? "#fff" : C.steel, borderRadius: 6, padding: "5px 10px", cursor: "pointer", fontFamily: MONO, fontSize: 11.5, fontWeight: 500 }}>{a}</button>
            ))}
          </div>
          <HoverBox as="button" title="Toàn màn hình" onClick={() => { void boxRef.current?.requestFullscreen?.(); }} style={ctlBtn} hoverStyle={ctlHover}><Icon name="maximize" size={17} stroke={1.8} /></HoverBox>
        </div>
      </div>
    </div>
  );
}
