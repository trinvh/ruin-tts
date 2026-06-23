import { useState } from "react";
import { C, FONT, MONO } from "../theme";
import { Icon } from "../icons";
import { HoverBox } from "../ui";
import { dubStatus } from "../tabs";
import { copyFile, desktopDir, saveAsDialog, showMessage } from "../../platform";
import { ORDER, type DubProjectHook } from "./useDubProject";
import type { EditorHistory } from "./useEditorHistory";

interface Props {
  title: string;
  onTitle: (v: string) => void;
  onTitleCommit: (v: string) => void;
  snap: boolean;
  onToggleSnap: () => void;
  dub: DubProjectHook;
  history: EditorHistory;
  historyOpen: boolean;
  onToggleHistory: () => void;
}

const histBtn = (active: boolean, enabled: boolean): React.CSSProperties => ({
  width: 32,
  height: 32,
  border: `1px solid ${active ? "rgba(146,136,224,.5)" : C.border}`,
  background: active ? "rgba(146,136,224,.16)" : "transparent",
  color: active ? C.purpleLt : enabled ? C.steel : C.muted5,
  borderRadius: 7,
  display: "grid",
  placeItems: "center",
  cursor: enabled ? "pointer" : "default",
  fontFamily: FONT,
});

export function TopBar({ title, onTitle, onTitleCommit, snap, onToggleSnap, dub, history, historyOpen, onToggleHistory }: Props) {
  const status = dub.detail?.project.status ?? "";
  const working = dub.busy || dub.autoRun;
  const st = dubStatus(status || "created");
  const prog = dub.detail?.project.progress ?? null;
  const progLabel = dub.detail?.project.progress_label ?? null;
  const pct = prog != null ? ` ${Math.round(prog * 100)}%` : "";
  // While a step runs, show what it's doing (+ %); otherwise the static label.
  const statusText = working
    ? `${progLabel || (dub.autoRun ? "Đang chạy…" : st.label)}${pct}`
    : st.label;
  const statusColor = working ? C.orange : status === "failed" ? C.pink : status === "done" ? C.teal : C.muted;

  const h = dub.info?.video?.height ?? null;
  const res = h ? (h >= 2160 ? "4K" : `${h}p`) : null;

  const synthDone = dub.reachedIdx >= ORDER.indexOf("synthesized");
  const canAuto = !!dub.detail && !working && !synthDone;
  const exporting = status === "exporting";
  const canExport = !!dub.detail && !working;

  const exportPath = dub.detail?.project.export_path ?? null;
  const [saveMsg, setSaveMsg] = useState("");
  const saveVideo = async () => {
    if (!exportPath) return;
    const safe = (dub.detail?.project.name || "video").replace(/[\\/:*?"<>|]/g, "_");
    const desk = await desktopDir();
    const dflt = desk ? `${desk.replace(/\/$/, "")}/${safe}.mp4` : `${safe}.mp4`;
    const dest = await saveAsDialog(dflt);
    if (!dest) return;
    setSaveMsg("Đang lưu…");
    const err = await copyFile(exportPath, dest);
    if (err) {
      setSaveMsg("Lưu thất bại");
      await showMessage(
        `Không lưu được video.\n\nNguồn:\n${exportPath}\n\nĐích:\n${dest}\n\nLỗi: ${err}`,
        "Lưu video thất bại",
        "error",
      );
    } else {
      setSaveMsg("Đã lưu ✓");
    }
    setTimeout(() => setSaveMsg(""), 3000);
  };

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
            onBlur={(e) => onTitleCommit(e.target.value)}
            spellCheck={false}
            style={{ background: "transparent", border: "1px solid transparent", borderRadius: 5, color: "#fff", fontFamily: FONT, fontSize: 14, fontWeight: 600, padding: "2px 6px", margin: "-2px -6px", maxWidth: 280, letterSpacing: "-.01em" }}
          />
          <span style={{ fontSize: 10.5, color: statusColor, fontWeight: 500, display: "flex", alignItems: "center", gap: 4 }}>
            {working ? (
              <span style={{ width: 10, height: 10, border: `2px solid ${statusColor}`, borderTopColor: "transparent", borderRadius: "50%", animation: "bss-spin .7s linear infinite", display: "inline-block" }} />
            ) : (
              <span style={{ width: 6, height: 6, borderRadius: "50%", background: statusColor, display: "inline-block" }} />
            )}
            {statusText}
            {dub.busy && (
              <button onClick={() => void dub.cancel()} style={{ marginLeft: 6, border: "none", background: "transparent", color: C.steel, fontSize: 10.5, textDecoration: "underline", cursor: "pointer", fontFamily: FONT }}>huỷ</button>
            )}
          </span>
        </div>
      </div>

      {/* snap */}
      <div style={{ flex: "none", display: "flex", alignItems: "center", gap: 4 }}>
        <button
          onClick={onToggleSnap}
          title="Bám dính"
          style={{ height: 32, padding: "0 11px", border: `1px solid ${snap ? "rgba(234,124,105,.5)" : C.border}`, background: snap ? "rgba(234,124,105,.16)" : "transparent", color: snap ? C.coral : C.steel, borderRadius: 7, display: "flex", alignItems: "center", gap: 7, cursor: "pointer", fontFamily: FONT, fontSize: 12.5, fontWeight: 500 }}
        >
          <svg viewBox="0 0 24 24" width={15} height={15} fill="currentColor">
            <path d="M5 3a1 1 0 0 0-1 1v7a8 8 0 0 0 16 0V4a1 1 0 0 0-1-1h-3a1 1 0 0 0-1 1v7a3 3 0 0 1-6 0V4a1 1 0 0 0-1-1z" />
            <path d="M4 7h4v3H4zM16 7h4v3h-4z" fill={C.panel} />
          </svg>
          Bám
        </button>

        <div style={{ width: 1, height: 20, background: C.border, margin: "0 4px" }} />
        <button onClick={() => history.canUndo && history.undo()} disabled={!history.canUndo} title="Hoàn tác (⌘Z)" style={histBtn(false, history.canUndo)}>
          <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M9 14L4 9l5-5" /><path d="M4 9h11a5 5 0 0 1 0 10h-1" /></svg>
        </button>
        <button onClick={() => history.canRedo && history.redo()} disabled={!history.canRedo} title="Làm lại (⇧⌘Z)" style={histBtn(false, history.canRedo)}>
          <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M15 14l5-5-5-5" /><path d="M20 9H9a5 5 0 0 0 0 10h1" /></svg>
        </button>
        <button onClick={onToggleHistory} title="Lịch sử chỉnh sửa" style={histBtn(historyOpen, true)}>
          <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M3 3v5h5" /><path d="M3.05 13A9 9 0 1 0 6 5.3L3 8" /><path d="M12 7v5l3 2" /></svg>
        </button>
      </div>

      {/* right: auto-dub / quality / export */}
      <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 10 }}>
        <HoverBox
          as="button"
          onClick={() => canAuto && void dub.runTo("synthesized")}
          title="Tự chạy tách giọng → phân tích → dịch → đọc TTS"
          style={{ height: 34, padding: "0 13px", border: "1px solid rgba(146,136,224,.5)", background: "rgba(146,136,224,.12)", color: C.purpleLt, borderRadius: 8, display: "flex", alignItems: "center", gap: 8, cursor: canAuto ? "pointer" : "default", opacity: canAuto ? 1 : 0.45, fontFamily: FONT, fontSize: 13, fontWeight: 600 }}
          hoverStyle={canAuto ? { background: "rgba(146,136,224,.22)", color: "#fff" } : undefined}
        >
          <Icon name="runAll" size={15} color="currentColor" />
          Lồng tiếng tự động
        </HoverBox>
        {res && (
          <div title="Độ phân giải nguồn — xuất ra MP4" style={{ height: 34, padding: "0 12px", border: `1px solid ${C.border}`, background: C.panel2, color: "#fff", borderRadius: 8, display: "flex", alignItems: "center", gap: 9, fontFamily: FONT }}>
            <span style={{ fontSize: 13, fontWeight: 600 }}>{res}</span>
            <span style={{ fontSize: 11, color: C.muted2, fontFamily: MONO, borderLeft: `1px solid ${C.border}`, paddingLeft: 9 }}>MP4</span>
          </div>
        )}
        <HoverBox
          as="button"
          onClick={() => canExport && void dub.run("export")}
          title="Ghép các track trên timeline thành video"
          style={{ height: 34, padding: "0 18px", border: "none", background: C.coral, color: "#fff", borderRadius: 8, display: "flex", alignItems: "center", gap: 8, cursor: canExport ? "pointer" : "default", opacity: canExport ? 1 : 0.55, fontFamily: FONT, fontSize: 13.5, fontWeight: 600, boxShadow: "0 4px 14px rgba(234,124,105,.4)" }}
          hoverStyle={canExport ? { background: C.coralLt } : undefined}
          activeStyle={canExport ? { transform: "scale(.97)" } : undefined}
        >
          <Icon name="export" size={16} stroke={1.9} />
          {exporting ? "Đang xuất…" : "Xuất video"}
        </HoverBox>
        {exportPath && (
          <HoverBox
            as="button"
            onClick={() => void saveVideo()}
            title="Lưu video đã xuất ra máy (mặc định Desktop)"
            style={{ height: 34, padding: "0 13px", border: `1px solid ${C.border}`, background: C.panel2, color: "#fff", borderRadius: 8, display: "flex", alignItems: "center", gap: 7, cursor: "pointer", fontFamily: FONT, fontSize: 13, fontWeight: 600 }}
            hoverStyle={{ background: C.panel3 }}
          >
            <Icon name="export" size={15} stroke={1.8} />
            {saveMsg || "Lưu video"}
          </HoverBox>
        )}
      </div>
    </div>
  );
}
