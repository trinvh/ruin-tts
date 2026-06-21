import { C, FONT, MONO } from "../theme";
import { Icon } from "../icons";
import { HoverBox } from "../ui";
import type { StudioActions, StudioState } from "./useStudio";
import type { Clip } from "./types";

interface Props {
  title: string;
  onTitle: (v: string) => void;
  state: StudioState;
  actions: StudioActions;
}

const iconBtn: React.CSSProperties = {
  width: 32, height: 32, border: "none", background: "transparent", color: C.steel,
  borderRadius: 7, display: "grid", placeItems: "center", cursor: "pointer",
};
const iconHover: React.CSSProperties = { background: C.panel3, color: "#fff" };

export function TopBar({ title, onTitle, state, actions }: Props) {
  const sel = state.clips.find((c: Clip) => c.id === state.sel);
  const hasDubVid = !!(sel && sel.type === "video");
  const snap = state.snap;

  return (
    <div style={{ height: 48, flex: "none", display: "flex", alignItems: "center", padding: "0 14px", gap: 12, background: C.panel, borderBottom: `1px solid ${C.border}` }}>
      {/* left: logo + title */}
      <div style={{ flex: 1, display: "flex", alignItems: "center", gap: 11, minWidth: 0 }}>
        <div style={{ width: 30, height: 30, flex: "none", borderRadius: 8, background: `linear-gradient(160deg,${C.purple},${C.purpleDk})`, display: "grid", placeItems: "center", boxShadow: "0 2px 8px rgba(146,136,224,.4)" }}>
          <Icon name="film" size={17} stroke={1.9} color="#fff" />
        </div>
        <div style={{ display: "flex", flexDirection: "column", minWidth: 0, gap: 1 }}>
          <input
            value={title}
            onChange={(e) => onTitle(e.target.value)}
            spellCheck={false}
            style={{ background: "transparent", border: "1px solid transparent", borderRadius: 5, color: "#fff", fontFamily: FONT, fontSize: 14, fontWeight: 600, padding: "2px 6px", margin: "-2px -6px", maxWidth: 280, letterSpacing: "-.01em" }}
          />
          <span style={{ fontSize: 10.5, color: C.teal, fontWeight: 500, display: "flex", alignItems: "center", gap: 4 }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: C.teal, display: "inline-block" }} />
            Đã lưu
          </span>
        </div>
      </div>

      {/* undo / redo / snap */}
      <div style={{ flex: "none", display: "flex", alignItems: "center", gap: 4 }}>
        <HoverBox as="button" title="Hoàn tác" style={iconBtn} hoverStyle={iconHover}>
          <Icon name="undo" size={17} />
        </HoverBox>
        <HoverBox as="button" title="Làm lại" style={iconBtn} hoverStyle={iconHover}>
          <Icon name="redo" size={17} />
        </HoverBox>
        <div style={{ width: 1, height: 22, background: C.border, margin: "0 6px" }} />
        <button
          onClick={actions.toggleSnap}
          title="Bám dính"
          style={{ height: 32, padding: "0 11px", border: `1px solid ${snap ? "rgba(234,124,105,.5)" : C.border}`, background: snap ? "rgba(234,124,105,.16)" : "transparent", color: snap ? C.coral : C.steel, borderRadius: 7, display: "flex", alignItems: "center", gap: 7, cursor: "pointer", fontFamily: FONT, fontSize: 12.5, fontWeight: 500 }}
        >
          <svg viewBox="0 0 24 24" width={15} height={15} fill="currentColor">
            <path d="M5 3a1 1 0 0 0-1 1v7a8 8 0 0 0 16 0V4a1 1 0 0 0-1-1h-3a1 1 0 0 0-1 1v7a3 3 0 0 1-6 0V4a1 1 0 0 0-1-1z" />
            <path d="M4 7h4v3H4zM16 7h4v3h-4z" fill={C.panel} />
          </svg>
          Bám
        </button>
      </div>

      {/* right: run-all / quality / export */}
      <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 10 }}>
        <HoverBox
          as="button"
          onClick={() => hasDubVid && actions.runAll()}
          title="Chạy tự động cả 4 bước cho video đang chọn"
          style={{ height: 34, padding: "0 13px", border: "1px solid rgba(146,136,224,.5)", background: "rgba(146,136,224,.12)", color: C.purpleLt, borderRadius: 8, display: "flex", alignItems: "center", gap: 8, cursor: hasDubVid ? "pointer" : "default", opacity: hasDubVid ? 1 : 0.45, fontFamily: FONT, fontSize: 13, fontWeight: 600 }}
          hoverStyle={hasDubVid ? { background: "rgba(146,136,224,.22)", color: "#fff" } : undefined}
        >
          <Icon name="runAll" size={15} color="currentColor" />
          Lồng tiếng tự động
        </HoverBox>
        <HoverBox
          as="button"
          style={{ height: 34, padding: "0 12px", border: `1px solid ${C.border}`, background: C.panel2, color: "#fff", borderRadius: 8, display: "flex", alignItems: "center", gap: 9, cursor: "pointer", fontFamily: FONT }}
          hoverStyle={{ borderColor: "#4a4e5e", background: C.panel3 }}
        >
          <span style={{ fontSize: 13, fontWeight: 600 }}>1080p</span>
          <span style={{ fontSize: 11, color: C.muted2, fontFamily: MONO, borderLeft: `1px solid ${C.border}`, paddingLeft: 9 }}>MP4</span>
        </HoverBox>
        <HoverBox
          as="button"
          style={{ height: 34, padding: "0 18px", border: "none", background: C.coral, color: "#fff", borderRadius: 8, display: "flex", alignItems: "center", gap: 8, cursor: "pointer", fontFamily: FONT, fontSize: 13.5, fontWeight: 600, boxShadow: "0 4px 14px rgba(234,124,105,.4)" }}
          hoverStyle={{ background: C.coralLt }}
          activeStyle={{ transform: "scale(.97)" }}
        >
          <Icon name="export" size={16} stroke={1.9} />
          Xuất video
        </HoverBox>
      </div>
    </div>
  );
}
