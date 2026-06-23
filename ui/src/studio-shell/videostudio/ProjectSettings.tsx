import { useState } from "react";
import { C, FONT } from "../theme";
import { HoverBox } from "../ui";
import type { DubProjectHook } from "./useDubProject";

interface Props {
  dub: DubProjectHook;
  onClose: () => void;
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  background: C.inset,
  border: `1px solid ${C.borderInset}`,
  borderRadius: 7,
  color: "#fff",
  fontSize: 12.5,
  padding: "8px 11px",
  outline: "none",
  fontFamily: FONT,
  boxSizing: "border-box",
};

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      <span style={{ fontSize: 11.5, color: C.muted2 }}>{label}</span>
      {children}
      {hint && <span style={{ fontSize: 10.5, color: C.muted4 }}>{hint}</span>}
    </label>
  );
}

/**
 * Per-project settings (overrides the global defaults). Currently: name, Gemini
 * model, and the diarization speaker cap. Leaving the cap blank inherits the
 * global default from the Settings page.
 */
export function ProjectSettings({ dub, onClose }: Props) {
  const p = dub.detail?.project;
  const [name, setName] = useState(p?.name ?? "");
  const [model, setModel] = useState(p?.gemini_model ?? "");
  const [maxSpk, setMaxSpk] = useState(p?.max_speakers != null ? String(p.max_speakers) : "");
  const [saving, setSaving] = useState(false);
  if (!p) return null;

  const save = async () => {
    setSaving(true);
    try {
      const n = parseInt(maxSpk, 10);
      await dub.patchSettings({
        gemini_model: model.trim() || p.gemini_model,
        max_speakers: Number.isFinite(n) && n > 0 ? n : null,
      });
      const nm = name.trim();
      if (nm && nm !== p.name) await dub.rename(nm);
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      onClick={onClose}
      style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,.5)", zIndex: 30, display: "grid", placeItems: "center" }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{ width: 430, maxWidth: "90%", background: C.panel, border: `1px solid ${C.border}`, borderRadius: 12, boxShadow: "0 20px 60px rgba(0,0,0,.55)", fontFamily: FONT, overflow: "hidden" }}
      >
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "13px 16px", borderBottom: `1px solid ${C.border}` }}>
          <span style={{ fontSize: 13.5, fontWeight: 600, color: "#fff" }}>Cài đặt dự án</span>
          <HoverBox as="button" onClick={onClose} style={{ width: 24, height: 24, border: "none", background: "transparent", color: C.muted3, borderRadius: 6, display: "grid", placeItems: "center", cursor: "pointer", fontSize: 16 }} hoverStyle={{ background: C.panel3, color: "#fff" }}>×</HoverBox>
        </div>

        <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 14 }}>
          <Field label="Tên dự án">
            <input style={inputStyle} value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} />
          </Field>
          <Field label="Model Gemini" hint="vd: gemini-2.5-flash, gemini-2.5-pro">
            <input style={inputStyle} value={model} onChange={(e) => setModel(e.target.value)} spellCheck={false} placeholder="gemini-2.5-flash" />
          </Field>
          <Field label="Số người nói tối đa" hint="để trống = theo cài đặt chung; áp dụng khi chạy lại bước Phân tích">
            <input
              style={inputStyle}
              type="number"
              min={0}
              value={maxSpk}
              onChange={(e) => setMaxSpk(e.target.value)}
              placeholder="theo cài đặt chung"
            />
          </Field>
        </div>

        <div style={{ padding: "12px 16px", borderTop: `1px solid ${C.border}`, display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <HoverBox as="button" onClick={onClose} style={{ border: `1px solid ${C.borderInset2}`, background: "transparent", color: C.muted2, borderRadius: 7, padding: "7px 14px", fontSize: 12.5, cursor: "pointer", fontFamily: FONT }} hoverStyle={{ background: C.panel3, color: "#fff" }}>Huỷ</HoverBox>
          <HoverBox as="button" onClick={saving ? undefined : save} style={{ border: "1px solid rgba(146,136,224,.45)", background: "rgba(146,136,224,.18)", color: C.purpleLt, borderRadius: 7, padding: "7px 16px", fontSize: 12.5, fontWeight: 600, cursor: saving ? "default" : "pointer", fontFamily: FONT, opacity: saving ? 0.6 : 1 }} hoverStyle={saving ? undefined : { background: "rgba(146,136,224,.28)" }}>{saving ? "Đang lưu…" : "Lưu"}</HoverBox>
        </div>
      </div>
    </div>
  );
}
