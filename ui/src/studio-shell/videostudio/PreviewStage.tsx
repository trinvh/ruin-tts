import { C, MONO } from "../theme";
import { Icon } from "../icons";
import { HoverBox } from "../ui";
import { fmt, totalDur } from "./constants";
import type { StudioActions, StudioState } from "./useStudio";
import type { Aspect, Clip } from "./types";

interface Props {
  state: StudioState;
  actions: StudioActions;
}

const ctlBtn: React.CSSProperties = { width: 32, height: 32, border: "none", background: "transparent", color: C.steel, borderRadius: 7, display: "grid", placeItems: "center", cursor: "pointer" };
const ctlHover: React.CSSProperties = { background: C.panel3, color: "#fff" };
const ASPECTS: Aspect[] = ["9:16", "1:1", "16:9"];

export function PreviewStage({ state, actions }: Props) {
  const { clips, playhead: ph, subStyle, aspect } = state;
  const TT = totalDur(clips);
  const vid = clips.find((c: Clip) => c.id === "vid");
  const f = clips.find((c) => c.type === "video" && c.start <= ph && ph < c.start + c.dur) ?? vid;

  const curVi = clips.find((c) => c.type === "sub" && c.lang === "vi" && c.start <= ph && ph < c.start + c.dur);
  const curZh = clips.find((c) => c.type === "sub" && c.lang === "zh" && c.start <= ph && ph < c.start + c.dur);
  const hasVi = clips.some((c) => c.lang === "vi");
  const capVi = curVi?.text ?? "";
  const capZh = curZh?.text ?? "";
  const showCaption = !!(capVi || (capZh && !hasVi));
  const showCapZh = !!(subStyle.bilingual && capZh && capVi);
  const img = clips.find((c) => c.type === "image" && c.start <= ph && ph < c.start + c.dur);

  const aspectCss = aspect === "9:16" ? "9 / 16" : aspect === "1:1" ? "1 / 1" : "16 / 9";

  if (!f) return <div style={{ flex: 1, background: C.previewBg }} />;

  const filter = `brightness(${(1 + (f.bri || 0) / 100).toFixed(3)}) contrast(${(1 + (f.con || 0) / 100).toFixed(3)}) saturate(${(1 + (f.sat || 0) / 100).toFixed(3)})`;
  const transform = `translateY(${((f.posY || 0) * 0.35).toFixed(1)}px) scale(${((f.scale || 100) / 100).toFixed(3)})`;

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
        <div ref={(el) => actions.setPrev(el)} style={{ position: "relative", height: "100%", aspectRatio: aspectCss, maxWidth: "100%", borderRadius: 6, overflow: "hidden", background: "#000", boxShadow: "0 10px 40px rgba(0,0,0,.55),0 0 0 1px rgba(255,255,255,.05)" }}>
          <img src={f.thumb} alt="" style={{ width: "100%", height: "100%", objectFit: "cover", display: "block", filter, transform, opacity: (f.opacity ?? 100) / 100 }} />
          {img && (
            <img
              onPointerDown={(e) => actions.imgDown(e)}
              src={img.thumb}
              alt=""
              style={{ position: "absolute", width: `${(34 * (img.scale ?? 100)) / 100}%`, left: `${img.ox ?? 62}%`, top: `${img.oy ?? 8}%`, borderRadius: 8, cursor: "grab", boxShadow: "0 4px 14px rgba(0,0,0,.5)", opacity: (img.opacity ?? 100) / 100 }}
            />
          )}
          {showCaption && (
            <div style={{ position: "absolute", left: "6%", right: "6%", top: `${subStyle.pos}%`, textAlign: "center" }}>
              {showCapZh && <div style={{ fontSize: 13, color: C.ink4, marginBottom: 4, textShadow: "0 1px 3px rgba(0,0,0,.9)" }}>{capZh}</div>}
              <div style={{ display: "inline-block", background: subStyle.bg ? "rgba(0,0,0,.55)" : "transparent", padding: "4px 12px", borderRadius: 7 }}>
                <span style={{ fontWeight: 700, color: subStyle.color, textShadow: "0 1px 4px rgba(0,0,0,.85)", fontSize: Math.round(subStyle.size * 0.62) }}>{capVi || capZh}</span>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* transport */}
      <div style={{ flex: "none", height: 52, display: "flex", alignItems: "center", padding: "0 16px", borderTop: `1px solid ${C.border}`, background: C.panel }}>
        <div style={{ flex: 1, display: "flex", alignItems: "center", gap: 8, fontFamily: MONO, fontSize: 13 }}>
          <span style={{ color: "#fff", fontWeight: 500 }}>{fmt(ph)}</span>
          <span style={{ color: C.muted5 }}>/</span>
          <span style={{ color: C.muted2 }}>{fmt(TT)}</span>
        </div>
        <div style={{ flex: "none", display: "flex", alignItems: "center", gap: 6 }}>
          <HoverBox as="button" onClick={actions.toStart} style={ctlBtn} hoverStyle={ctlHover}>
            <Icon name="toStart" size={18} />
          </HoverBox>
          <HoverBox
            as="button"
            onClick={actions.togglePlay}
            style={{ width: 40, height: 40, border: "none", background: C.coral, color: "#fff", borderRadius: "50%", display: "grid", placeItems: "center", cursor: "pointer", boxShadow: "0 4px 14px rgba(234,124,105,.45)" }}
            hoverStyle={{ background: C.coralLt }}
            activeStyle={{ transform: "scale(.95)" }}
          >
            {state.playing ? <Icon name="pause" size={17} /> : <Icon name="play" size={18} style={{ marginLeft: 2 }} />}
          </HoverBox>
          <HoverBox as="button" style={ctlBtn} hoverStyle={ctlHover}>
            <Icon name="toEnd" size={18} />
          </HoverBox>
        </div>
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 10 }}>
          <div style={{ display: "flex", background: C.panel2, border: `1px solid ${C.border}`, borderRadius: 8, padding: 2, gap: 2 }}>
            {ASPECTS.map((a) => (
              <button key={a} onClick={() => actions.setAspect(a)} style={{ border: "none", background: a === aspect ? C.coral : "transparent", color: a === aspect ? "#fff" : C.steel, borderRadius: 6, padding: "5px 10px", cursor: "pointer", fontFamily: MONO, fontSize: 11.5, fontWeight: 500 }}>{a}</button>
            ))}
          </div>
          <HoverBox as="button" style={ctlBtn} hoverStyle={ctlHover}>
            <Icon name="maximize" size={17} stroke={1.8} />
          </HoverBox>
        </div>
      </div>
    </div>
  );
}
