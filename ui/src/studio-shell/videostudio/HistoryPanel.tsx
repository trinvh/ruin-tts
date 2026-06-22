import { C, FONT, MONO } from "../theme";
import { HoverBox } from "../ui";
import type { EditorHistory } from "./useEditorHistory";

interface Props {
  history: EditorHistory;
  onClose: () => void;
}

/** Floating panel listing every track edit; click an entry to jump to that exact state. */
export function HistoryPanel({ history, onClose }: Props) {
  return (
    <div
      style={{
        position: "absolute", top: 8, right: 8, width: 248, maxHeight: "70%",
        display: "flex", flexDirection: "column",
        background: C.panel, border: `1px solid ${C.border}`, borderRadius: 10,
        boxShadow: "0 12px 40px rgba(0,0,0,.5)", zIndex: 20, overflow: "hidden",
      }}
    >
      <div style={{ flex: "none", display: "flex", alignItems: "center", justifyContent: "space-between", padding: "10px 12px", borderBottom: `1px solid ${C.border}` }}>
        <span style={{ fontSize: 12.5, fontWeight: 600, color: "#fff", fontFamily: FONT }}>Lịch sử chỉnh sửa</span>
        <HoverBox as="button" onClick={onClose} style={{ width: 22, height: 22, border: "none", background: "transparent", color: C.muted3, borderRadius: 5, display: "grid", placeItems: "center", cursor: "pointer", fontSize: 15 }} hoverStyle={{ background: C.panel3, color: "#fff" }}>×</HoverBox>
      </div>
      <div style={{ flex: 1, overflowY: "auto", padding: 6 }}>
        {history.entries.length === 0 && (
          <div style={{ padding: 12, fontSize: 11.5, color: C.muted3, textAlign: "center", fontFamily: FONT }}>Chưa có thay đổi nào.</div>
        )}
        {history.entries.map((e, i) => {
          const active = i === history.cursor;
          const future = i > history.cursor;
          return (
            <HoverBox
              key={i}
              onClick={() => history.jumpTo(i)}
              style={{
                display: "flex", alignItems: "center", gap: 8, padding: "7px 9px", borderRadius: 7,
                cursor: "pointer", marginBottom: 2,
                background: active ? "rgba(146,136,224,.16)" : "transparent",
                opacity: future ? 0.45 : 1,
              }}
              hoverStyle={active ? undefined : { background: "#201e2a" }}
            >
              <span style={{ width: 7, height: 7, borderRadius: "50%", flex: "none", background: active ? C.purpleLt : C.muted5 }} />
              <span style={{ flex: 1, fontSize: 12, color: active ? "#fff" : C.ink3, fontFamily: FONT, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{e.label}</span>
              <span style={{ fontSize: 9.5, color: C.muted5, fontFamily: MONO }}>{i}</span>
            </HoverBox>
          );
        })}
      </div>
    </div>
  );
}
