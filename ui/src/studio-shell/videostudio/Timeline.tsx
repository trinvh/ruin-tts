import { C, FONT, MONO } from "../theme";
import { Icon } from "../icons";
import { HoverBox } from "../ui";
import { bars, clipColors, fmt, totalDur, TRACK_ORDER, trackDot, trackLabel } from "./constants";
import type { StudioActions, StudioState } from "./useStudio";
import type { Clip } from "./types";

interface Props {
  state: StudioState;
  actions: StudioActions;
}

const stop = (e: React.MouseEvent) => e.stopPropagation();
const toolBtn: React.CSSProperties = { height: 26, padding: "0 9px", border: "none", background: "transparent", borderRadius: 6, display: "flex", alignItems: "center", gap: 6, fontFamily: FONT, fontSize: 12 };
const zoomBtn: React.CSSProperties = { width: 26, height: 26, border: "none", background: C.panel2, color: C.steel, borderRadius: 6, display: "grid", placeItems: "center", cursor: "pointer" };

export function Timeline({ state, actions }: Props) {
  const { clips, sel, playhead: ph } = state;
  const TT = totalDur(clips);
  const selClip = clips.find((c: Clip) => c.id === sel) ?? null;
  const dubSel = selClip && selClip.type === "video" ? selClip.id : null;
  const tachDone = dubSel ? actions.getDub(dubSel).pipe.tach === "done" : false;

  const present = [...new Set(clips.map((c) => c.track))].sort((a, b) => (TRACK_ORDER[a] ?? 9) - (TRACK_ORDER[b] ?? 9));
  const ticks: { left: string; label: string }[] = [];
  for (let t = 0; t <= TT; t++) ticks.push({ left: ((t / TT) * 100).toFixed(2), label: fmt(t).slice(3) });

  const splitOn = !!selClip;
  const delOn = !!(selClip && selClip.id !== "vid");

  return (
    <div style={{ height: "33vh", flex: "none", background: C.panel, borderTop: `1px solid ${C.border}`, display: "flex", flexDirection: "column", minHeight: 0 }}>
      {/* header */}
      <div style={{ flex: "none", height: 36, display: "flex", alignItems: "center", padding: "0 12px", gap: 10, borderBottom: `1px solid ${C.borderSoft}` }}>
        <span style={{ fontSize: 11, fontWeight: 600, letterSpacing: ".08em", textTransform: "uppercase", color: C.muted2 }}>Timeline</span>
        <div style={{ width: 1, height: 16, background: C.border }} />
        <HoverBox as="button" onClick={() => splitOn && actions.splitSel()} style={{ ...toolBtn, color: splitOn ? C.steel : "#4a4e5e", cursor: splitOn ? "pointer" : "default" }} hoverStyle={splitOn ? { background: C.panel3, color: "#fff" } : undefined}>
          <Icon name="split" size={15} /> Cắt
        </HoverBox>
        <HoverBox as="button" onClick={() => delOn && actions.delSel()} style={{ ...toolBtn, color: delOn ? C.steel : "#4a4e5e", cursor: delOn ? "pointer" : "default" }} hoverStyle={delOn ? { background: C.panel3, color: C.pink } : undefined}>
          <Icon name="trash" size={15} /> Xoá
        </HoverBox>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11, color: C.muted2, fontFamily: MONO }}>{fmt(ph)}</span>
      </div>

      {/* body: gutter + lanes */}
      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        <div className="noscroll" style={{ width: 128, flex: "none", borderRight: `1px solid ${C.border}`, background: C.inset, overflowY: "hidden" }}>
          <div style={{ height: 26, borderBottom: `1px solid ${C.borderSoft}` }} />
          {present.map((k) => (
            <div key={k} style={{ height: 46, borderBottom: `1px solid ${C.borderSoft}`, display: "flex", alignItems: "center", padding: "0 8px 0 11px", gap: 5 }}>
              <span style={{ width: 18, height: 18, flex: "none", borderRadius: 5, background: trackDot(k), display: "inline-block" }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11.5, fontWeight: 600, color: C.ink5, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{trackLabel(k, tachDone)}</div>
              </div>
              <HoverBox as="button" title="Ẩn/hiện" style={{ width: 20, height: 20, border: "none", background: "transparent", color: C.muted5, borderRadius: 5, display: "grid", placeItems: "center", cursor: "pointer" }} hoverStyle={{ color: C.steel }}>
                <Icon name="eye" size={12} stroke={1.8} />
              </HoverBox>
            </div>
          ))}
        </div>

        <div ref={(el) => actions.setLane(el)} onClick={() => actions.deselect()} style={{ flex: 1, position: "relative", overflowY: "auto", overflowX: "hidden", background: C.laneBg }}>
          {/* ruler */}
          <div onPointerDown={(e) => actions.rulerDown(e)} onClick={stop} style={{ height: 26, position: "relative", borderBottom: `1px solid ${C.borderSoft}`, cursor: "ew-resize", background: C.ruler }}>
            {ticks.map((tk, i) => (
              <div key={i} style={{ position: "absolute", top: 0, bottom: 0, left: `${tk.left}%`, borderLeft: `1px solid ${C.tick}`, paddingLeft: 5, display: "flex", alignItems: "center" }}>
                <span style={{ fontSize: 9.5, color: C.muted3, fontFamily: MONO }}>{tk.label}</span>
              </div>
            ))}
          </div>

          {present.map((k) => (
            <div key={k} style={{ height: 46, position: "relative", borderBottom: `1px solid ${C.borderSoft}`, background: "transparent" }}>
              {clips.filter((c) => c.track === k).map((c) => {
                const col = clipColors(c);
                const isSel = c.id === sel;
                return (
                  <div
                    key={c.id}
                    onPointerDown={actions.clipDown(c.id, "move")}
                    onClick={stop}
                    style={{
                      position: "absolute", top: 4, bottom: 4, left: `${((c.start / TT) * 100).toFixed(2)}%`, width: `${((c.dur / TT) * 100).toFixed(2)}%`,
                      borderRadius: 5, border: isSel ? `2px solid ${C.coral}` : "1px solid rgba(255,255,255,.08)",
                      boxShadow: isSel ? `0 0 0 1px ${C.coral},0 4px 14px rgba(234,124,105,.4)` : "none",
                      overflow: "hidden", cursor: "grab", background: col.bg, backgroundImage: col.bgImg, backgroundSize: "cover", backgroundPosition: "center", display: "flex", alignItems: "center",
                    }}
                  >
                    {col.isAudio && (
                      <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", gap: 1.5, padding: "0 7px", opacity: 0.6 }}>
                        {bars(c.start * 3, 14).map((b, i) => (
                          <div key={i} style={{ flex: 1, height: b.h, background: col.wave, borderRadius: 1, minWidth: 1 }} />
                        ))}
                      </div>
                    )}
                    <div style={{ position: "absolute", inset: 0, background: col.scrim }} />
                    <span style={{ position: "relative", padding: "0 8px", fontSize: 10.5, fontWeight: 600, color: col.textColor, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", textShadow: "0 1px 2px rgba(0,0,0,.6)" }}>{c.name}</span>
                    {isSel && (
                      <>
                        <div onPointerDown={actions.clipDown(c.id, "l")} style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 7, background: C.coral, cursor: "ew-resize" }} />
                        <div onPointerDown={actions.clipDown(c.id, "r")} style={{ position: "absolute", right: 0, top: 0, bottom: 0, width: 7, background: C.coral, cursor: "ew-resize" }} />
                      </>
                    )}
                  </div>
                );
              })}
            </div>
          ))}

          <div style={{ position: "absolute", top: 0, bottom: 0, left: `${((ph / TT) * 100).toFixed(2)}%`, width: 0, borderLeft: `2px solid ${C.coral}`, pointerEvents: "none", zIndex: 5, boxShadow: "0 0 8px rgba(234,124,105,.6)" }}>
            <div style={{ position: "absolute", top: -1, left: -7, width: 14, height: 13, background: C.coral, clipPath: "polygon(0 0,100% 0,100% 55%,50% 100%,0 55%)" }} />
          </div>
        </div>
      </div>

      {/* footer: zoom */}
      <div style={{ flex: "none", height: 34, display: "flex", alignItems: "center", padding: "0 12px", gap: 8, borderTop: `1px solid ${C.borderSoft}` }}>
        <HoverBox as="button" onClick={actions.zoomOut} style={zoomBtn} hoverStyle={{ background: C.panel3, color: "#fff" }}>
          <Icon name="zoomOut" size={15} stroke={1.9} />
        </HoverBox>
        <span style={{ fontSize: 11, color: C.muted2, fontFamily: MONO, width: 42, textAlign: "center" }}>{state.zoom}%</span>
        <HoverBox as="button" onClick={actions.zoomIn} style={zoomBtn} hoverStyle={{ background: C.panel3, color: "#fff" }}>
          <Icon name="zoomIn" size={15} stroke={1.9} />
        </HoverBox>
      </div>
    </div>
  );
}
