// Real px/second, scrollable, zoomable timeline backed by
// @xzdarcy/react-timeline-editor. Drop-in replacement for ./Timeline.tsx —
// same props (plus `onClipTrim`). The library renders only the editor area
// (ruler + lanes + cursor); we render our own left gutter and keep its vertical
// scroll in sync via the editor's `onScroll` callback.

import { useEffect, useMemo, useRef } from "react";
import { Timeline as RTLTimeline, type TimelineState } from "@xzdarcy/react-timeline-editor";
// The editor re-exports its own `TimelineState`, but the engine row/action/effect
// types live in @xzdarcy/timeline-engine (a transitive dep). We re-declare the
// minimal shapes we use rather than importing across that boundary; structural
// typing keeps them compatible with the library's expected props.
interface TimelineAction {
  id: string;
  start: number;
  end: number;
  effectId: string;
  selected?: boolean;
  flexible?: boolean;
  movable?: boolean;
}
interface TimelineRow {
  id: string;
  actions: TimelineAction[];
  rowHeight?: number;
}
interface TimelineEffect {
  id: string;
  name?: string;
}
import { C, FONT, MONO } from "../theme";
import { Icon } from "../icons";
import { HoverBox } from "../ui";
import { bars, clipColors, fmt, totalDur, TRACK_ORDER, trackDot, trackLabel } from "./constants";
import type { StudioActions, StudioState } from "./useStudio";
import type { Transport } from "./useTransport";
import type { TrackCtl } from "./trackmap";
import type { Clip } from "./types";

interface Props {
  state: StudioState;
  actions: StudioActions;
  transport: Transport;
  trackCtl: TrackCtl;
  /** Persist a clip's new range after a drag/resize (parent re-seeds clips). */
  onClipTrim?: (clipId: string, start: number, dur: number) => void;
  /** Pick + add a media clip (video/audio/image). */
  onAddMedia?: () => void;
  /** Delete a clip (parent decides what's deletable). */
  onDeleteClip?: (clipId: string) => void;
}

// ── layout constants (gutter + editor rows must agree) ──
const GUTTER_W = 128;
const ROW_H = 46;
const RULER_H = 32; // must match the library's .timeline-editor-time-area height
const START_LEFT = 12; // px the lanes start in from the editor's left edge
const TICK_SECONDS = 5; // seconds per major tick
const MIN_PX_PER_SEC = 1; // floor so very low zoom stays usable

const toolBtn: React.CSSProperties = { height: 26, padding: "0 9px", border: "none", background: "transparent", borderRadius: 6, display: "flex", alignItems: "center", gap: 6, fontFamily: FONT, fontSize: 12 };
const zoomBtn: React.CSSProperties = { width: 26, height: 26, border: "none", background: C.panel2, color: C.steel, borderRadius: 6, display: "grid", placeItems: "center", cursor: "pointer" };

// Effects map (one per clip type) — the library requires every action.effectId
// to resolve to an effect; the render itself is fully custom via getActionRender.
const EFFECTS: Record<string, TimelineEffect> = {
  video: { id: "video", name: "Video" },
  audio: { id: "audio", name: "Audio" },
  image: { id: "image", name: "Image" },
  sub: { id: "sub", name: "Subtitle" },
};

export function TimelineEditor({ state, actions, transport, trackCtl, onClipTrim, onAddMedia, onDeleteClip }: Props) {
  const { clips, sel } = state;
  const ph = transport.time;
  const TT = Math.max(transport.duration || 0, totalDur(clips));
  const selClip = clips.find((c: Clip) => c.id === sel) ?? null;
  const tachDone = clips.some((c) => c.track === "A1" && c.kind === "vocals");

  // Tracks present, in canonical order.
  const present = useMemo(
    () => [...new Set(clips.map((c) => c.track))].sort((a, b) => (TRACK_ORDER[a] ?? 9) - (TRACK_ORDER[b] ?? 9)),
    [clips],
  );

  // px/second + tick width derived from zoom (zoom 100 ≈ 10 px/sec).
  const pxPerSec = Math.max(MIN_PX_PER_SEC, state.zoom / 10);
  const scaleWidth = TICK_SECONDS * pxPerSec;
  // Enough ticks to cover content + a bit of empty headroom after the last clip.
  const minScaleCount = Math.ceil(TT / TICK_SECONDS) + 4;

  // Build editor rows from the present tracks (one row per track).
  const editorData: TimelineRow[] = useMemo(
    () =>
      present.map((k) => ({
        id: k,
        actions: clips
          .filter((c) => c.track === k)
          .map((c) => {
            // Original/music audio follow the video and stay locked. The video
            // clip moves (= lead-in), and dub lines / subtitles / banners move.
            const locked = c.track === "A1" || c.track === "A2";
            return {
              id: c.id,
              start: c.start,
              end: c.start + c.dur,
              effectId: c.type,
              movable: !locked,
              // Only banners resize (their time range); dub lines keep their
              // fixed duration and just shift.
              flexible: c.type === "image" && c.id === sel,
            };
          }),
      })),
    [present, clips, sel],
  );

  const clipById = useMemo(() => {
    const m = new Map<string, Clip>();
    for (const c of clips) m.set(c.id, c);
    return m;
  }, [clips]);

  // ── refs ──
  const timelineRef = useRef<TimelineState>(null);
  const gutterRef = useRef<HTMLDivElement | null>(null);

  // Drive the library cursor from the real transport time.
  useEffect(() => {
    timelineRef.current?.setTime(ph);
  }, [ph]);

  const commitFromAction = (action: TimelineAction) => {
    const start = Math.max(0, action.start);
    const dur = Math.max(0.1, action.end - action.start);
    onClipTrim?.(action.id, start, dur);
  };

  const splitOn = !!selClip;
  const delOn = !!selClip;

  return (
    <div style={{ height: "33vh", flex: "none", background: C.panel, borderTop: `1px solid ${C.border}`, display: "flex", flexDirection: "column", minHeight: 0, userSelect: "none", WebkitUserSelect: "none" }}>
      {/* header */}
      <div style={{ flex: "none", height: 36, display: "flex", alignItems: "center", padding: "0 12px", gap: 10, borderBottom: `1px solid ${C.borderSoft}` }}>
        <span style={{ fontSize: 11, fontWeight: 600, letterSpacing: ".08em", textTransform: "uppercase", color: C.muted2 }}>Timeline</span>
        <div style={{ width: 1, height: 16, background: C.border }} />
        <HoverBox as="button" onClick={() => onAddMedia?.()} style={{ ...toolBtn, color: C.purpleLt, cursor: "pointer" }} hoverStyle={{ background: "rgba(146,136,224,.16)", color: "#fff" }}>
          <Icon name="plus" size={15} /> Thêm media
        </HoverBox>
        <HoverBox as="button" onClick={() => splitOn && actions.splitSel()} style={{ ...toolBtn, color: splitOn ? C.steel : "#4a4e5e", cursor: splitOn ? "pointer" : "default" }} hoverStyle={splitOn ? { background: C.panel3, color: "#fff" } : undefined}>
          <Icon name="split" size={15} /> Cắt
        </HoverBox>
        <HoverBox as="button" onClick={() => delOn && sel && onDeleteClip?.(sel)} style={{ ...toolBtn, color: delOn ? C.steel : "#4a4e5e", cursor: delOn ? "pointer" : "default" }} hoverStyle={delOn ? { background: C.panel3, color: C.pink } : undefined}>
          <Icon name="trash" size={15} /> Xoá
        </HoverBox>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11, color: C.muted2, fontFamily: MONO }}>{fmt(ph)}</span>
      </div>

      {/* body: gutter + editor */}
      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        {/* left gutter — track labels, synced to editor vertical scroll */}
        <div ref={gutterRef} className="noscroll" style={{ width: GUTTER_W, flex: "none", borderRight: `1px solid ${C.border}`, background: C.inset, overflowY: "hidden" }}>
          <div style={{ height: RULER_H, borderBottom: `1px solid ${C.borderSoft}` }} />
          {present.map((k) => {
            const selectedTrack = state.sel === "track:" + k;
            const hasEye = trackCtl.hasEye(k);
            const on = trackCtl.enabled(k);
            return (
              <div
                key={k}
                onClick={() => actions.selectTrack(k)}
                style={{ height: ROW_H, borderBottom: `1px solid ${C.borderSoft}`, display: "flex", alignItems: "center", padding: "0 8px 0 11px", gap: 5, cursor: "pointer", background: selectedTrack ? "rgba(234,124,105,.12)" : "transparent", boxShadow: selectedTrack ? `inset 2px 0 0 0 ${C.coral}` : "none", opacity: on ? 1 : 0.7 }}
              >
                <span style={{ width: 18, height: 18, flex: "none", borderRadius: 5, background: trackDot(k), display: "inline-block", opacity: on ? 1 : 0.35 }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11.5, fontWeight: 600, color: on ? C.ink5 : C.muted4, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{trackLabel(k, tachDone)}</div>
                </div>
                {hasEye && (
                  <HoverBox
                    as="button"
                    title={on ? "Tắt track (preview + xuất)" : "Bật track"}
                    onClick={(e: React.MouseEvent) => { e.stopPropagation(); trackCtl.toggle(k); }}
                    style={{ width: 20, height: 20, border: "none", background: "transparent", color: on ? C.steel : C.muted5, opacity: on ? 1 : 0.5, borderRadius: 5, display: "grid", placeItems: "center", cursor: "pointer" }}
                    hoverStyle={{ color: "#fff" }}
                  >
                    <Icon name="eye" size={12} stroke={1.8} />
                  </HoverBox>
                )}
              </div>
            );
          })}
        </div>

        {/* editor area */}
        <div onClick={() => actions.deselect()} style={{ flex: 1, minWidth: 0, position: "relative", background: C.laneBg }}>
          <RTLTimeline
            ref={timelineRef}
            editorData={editorData}
            effects={EFFECTS}
            scale={TICK_SECONDS}
            scaleWidth={scaleWidth}
            scaleSplitCount={TICK_SECONDS}
            minScaleCount={minScaleCount}
            startLeft={START_LEFT}
            rowHeight={ROW_H}
            gridSnap={state.snap}
            dragLine={state.snap}
            autoScroll
            autoReRender
            style={{ width: "100%", height: "100%", background: "transparent" }}
            // keep the gutter's vertical scroll aligned with the editor
            onScroll={({ scrollTop }) => {
              if (gutterRef.current) gutterRef.current.scrollTop = scrollTop;
            }}
            getScaleRender={(scale: number) => (
              <span style={{ fontSize: 9.5, color: C.muted3, fontFamily: MONO }}>{fmt(scale).slice(3)}</span>
            )}
            getActionRender={(action: TimelineAction) => {
              const clip = clipById.get(action.id);
              if (!clip) return null;
              const col = clipColors(clip);
              const isSel = clip.id === sel;
              const trackSel = sel === "track:" + clip.track;
              const isAudio = clip.type === "audio";
              return (
                <div
                  style={{
                    position: "absolute", inset: "4px 0", borderRadius: 5,
                    border: isSel ? `2px solid ${C.coral}` : trackSel ? `1px solid ${C.coral}` : "1px solid rgba(255,255,255,.08)",
                    boxShadow: isSel ? `0 0 0 1px ${C.coral},0 4px 14px rgba(234,124,105,.4)` : trackSel ? "0 0 0 1px rgba(234,124,105,.4)" : "none",
                    overflow: "hidden", cursor: isAudio ? "pointer" : "grab",
                    background: col.bg, backgroundImage: col.bgImg, backgroundSize: "cover", backgroundPosition: "center",
                    display: "flex", alignItems: "center",
                  }}
                >
                  {col.isAudio && (
                    <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", gap: 1.5, padding: "0 7px", opacity: 0.6 }}>
                      {bars(clip.start * 3, 14).map((b, i) => (
                        <div key={i} style={{ flex: 1, height: b.h, background: col.wave, borderRadius: 1, minWidth: 1 }} />
                      ))}
                    </div>
                  )}
                  <div style={{ position: "absolute", inset: 0, background: col.scrim }} />
                  <span style={{ position: "relative", padding: "0 8px", fontSize: 10.5, fontWeight: 600, color: col.textColor, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", textShadow: "0 1px 2px rgba(0,0,0,.6)" }}>{clip.name}</span>
                  {/* resize affordances only when selected (the library handles the actual drag) */}
                  {isSel && !isAudio && (
                    <>
                      <div style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 7, background: C.coral, cursor: "ew-resize", pointerEvents: "none" }} />
                      <div style={{ position: "absolute", right: 0, top: 0, bottom: 0, width: 7, background: C.coral, cursor: "ew-resize", pointerEvents: "none" }} />
                    </>
                  )}
                </div>
              );
            }}
            onClickAction={(_e, { action }) => {
              const clip = clipById.get(action.id);
              if (!clip) return;
              if (clip.type === "audio") actions.selectTrack(clip.track);
              else actions.clipDown(clip.id, "move")(syntheticPointer());
            }}
            onClickRow={(_e, { row }) => actions.selectTrack(row.id)}
            onClickTimeArea={(time) => {
              transport.seek(Math.max(0, Math.min(TT, time)));
              return true;
            }}
            onCursorDragEnd={(time) => transport.seek(Math.max(0, Math.min(TT, time)))}
            onActionMoveEnd={({ action }) => commitFromAction(action)}
            onActionResizeEnd={({ action }) => commitFromAction(action)}
          />
        </div>
      </div>

      {/* footer: zoom */}
      <div style={{ flex: "none", height: 34, display: "flex", alignItems: "center", padding: "0 12px", gap: 8, borderTop: `1px solid ${C.borderSoft}` }}>
        <HoverBox as="button" onClick={actions.zoomOut} style={zoomBtn} hoverStyle={{ background: C.panel3, color: "#fff" }}><Icon name="zoomOut" size={15} stroke={1.9} /></HoverBox>
        <span style={{ fontSize: 11, color: C.muted2, fontFamily: MONO, width: 42, textAlign: "center" }}>{state.zoom}%</span>
        <HoverBox as="button" onClick={actions.zoomIn} style={zoomBtn} hoverStyle={{ background: C.panel3, color: "#fff" }}><Icon name="zoomIn" size={15} stroke={1.9} /></HoverBox>
      </div>
    </div>
  );
}

/** Minimal React.PointerEvent stand-in for selection-only clipDown calls. */
function syntheticPointer(): React.PointerEvent {
  return { stopPropagation: () => {}, clientX: 0, clientY: 0 } as unknown as React.PointerEvent;
}
