import { useEffect, useState } from "react";
import { C, FONT, MONO } from "../theme";
import { Icon, type IconName } from "../icons";
import { SUBC, trackLabel } from "./constants";
import { trackAudioKind, type TrackCtl } from "./trackmap";
import type { StudioActions, StudioState } from "./useStudio";
import type { DubProjectHook } from "./useDubProject";
import type { Transport } from "./useTransport";
import type { Clip } from "./types";

interface Props {
  state: StudioState;
  actions: StudioActions;
  dub: DubProjectHook;
  transport: Transport;
  trackCtl: TrackCtl;
}

const SECTION: React.CSSProperties = { fontSize: 11, fontWeight: 600, letterSpacing: ".08em", textTransform: "uppercase", color: C.muted2, marginBottom: 13 };
const cl = (v: number, mn: number, mx: number) => (((v - mn) / (mx - mn)) * 100).toFixed(1);

function Slider({ label, labelW = 58, min, max, value, onChange, onCommit, display, mode = "left" }: {
  label: string;
  labelW?: number;
  min: number;
  max: number;
  value: number;
  onChange: (v: number) => void;
  /** Fired on pointer/mouse up, after dragging — used to persist the value. */
  onCommit?: (v: number) => void;
  display: string;
  mode?: "left" | "center";
}) {
  const fill =
    mode === "center"
      ? `linear-gradient(to right,${C.border} 50%,${C.coral} 50%)`
      : `linear-gradient(to right,${C.coral} ${cl(value, min, max)}%,${C.border} ${cl(value, min, max)}%)`;
  const commit = (e: React.PointerEvent<HTMLInputElement> | React.MouseEvent<HTMLInputElement>) =>
    onCommit?.(Number((e.currentTarget as HTMLInputElement).value));
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 13 }}>
      <span style={{ width: labelW, flex: "none", fontSize: 12, color: C.steel }}>{label}</span>
      <input type="range" min={min} max={max} value={value} onChange={(e) => onChange(Number(e.target.value))} onPointerUp={onCommit ? commit : undefined} onMouseUp={onCommit ? commit : undefined} style={{ flex: 1, background: fill }} />
      <span style={{ width: 42, textAlign: "right", color: "#fff", fontFamily: MONO, fontSize: 12 }}>{display}</span>
    </div>
  );
}

function Toggle({ on, onClick }: { on: boolean; onClick: () => void }) {
  return (
    <button onClick={onClick} style={{ width: 42, height: 24, flex: "none", borderRadius: 12, border: "none", background: on ? C.purple : C.border, cursor: "pointer", position: "relative", transition: "background .15s" }}>
      <span style={{ position: "absolute", top: 2, left: on ? 20 : 2, width: 20, height: 20, borderRadius: "50%", background: "#fff", transition: "left .15s" }} />
    </button>
  );
}

const Divider = () => <div style={{ height: 1, background: C.border, margin: "18px 0" }} />;

export function Inspector({ state, actions, dub, transport, trackCtl }: Props) {
  const isTrack = !!state.sel?.startsWith("track:");
  const trackKey = isTrack ? state.sel!.slice(6) : null;
  const sel = !isTrack ? (state.clips.find((c: Clip) => c.id === state.sel) ?? null) : null;
  // The selected clip's backing dub_clip (for routing edits like subtitle text).
  const selClip = sel ? dub.detail?.clips.find((c) => c.id === sel.id) : undefined;
  const subSegId =
    sel && sel.type === "sub" && selClip?.origin.startsWith("dub:sub:")
      ? selClip.origin.slice("dub:sub:".length)
      : null;
  const commitSub = (text: string) => {
    if (!subSegId) return;
    const seg = dub.detail?.segments.find((s) => s.id === subSegId);
    void dub.setSegment(subSegId, text, seg?.voice ?? null);
  };
  // Persist a property change of a selected USER clip (volume/opacity).
  const isUserClip = selClip?.origin === "user";
  const commitClip = (patch: { volume?: number; opacity?: number }) => {
    if (!selClip || selClip.origin !== "user") return;
    void dub.patchClip(selClip.id, {
      track: selClip.track, start_s: selClip.start_s, dur_s: selClip.dur_s, in_s: selClip.in_s,
      volume: selClip.volume, x: selClip.x, y: selClip.y, w: selClip.w, opacity: selClip.opacity,
      text: selClip.text, text_style: selClip.text_style, ...patch,
    });
  };

  let name = "—";
  let kind = "";
  let tile: string = C.panel3;
  let color: string = C.muted;
  let icon: IconName = "film";
  if (trackKey) {
    const ak = trackAudioKind(trackKey);
    name = trackLabel(trackKey, state.clips.some((c) => c.track === "A1" && c.kind === "vocals"));
    kind = "Track";
    icon = ak === "sub" ? "subtitle" : "speaker";
    tile = ak === "original" ? "rgba(101,176,246,.16)" : "rgba(146,136,224,.16)";
    color = ak === "original" ? C.blue : C.purpleLt;
  } else if (sel) {
    name = sel.name;
    if (sel.type === "video") { kind = "Video · " + sel.dur.toFixed(1) + "s"; tile = "rgba(234,124,105,.16)"; color = C.coral; icon = "film"; }
    else if (sel.type === "image") { kind = "Hình ảnh"; tile = "rgba(101,176,246,.16)"; color = C.blue; icon = "image"; }
    else if (sel.type === "audio") { kind = (sel.kind === "tts" ? "Lồng tiếng" : sel.kind === "music" ? "Nhạc nền" : "Âm thanh") + " · " + sel.dur.toFixed(1) + "s"; tile = "rgba(146,136,224,.16)"; color = C.purpleLt; icon = "speaker"; }
    else { kind = sel.lang === "vi" ? "Phụ đề Việt" : "Phụ đề gốc"; tile = "rgba(146,136,224,.16)"; color = C.purpleLt; icon = "subtitle"; }
  }

  return (
    <div style={{ width: 300, flex: "none", background: C.panel, borderLeft: `1px solid ${C.border}`, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div style={{ flex: "none", padding: "13px 16px", borderBottom: `1px solid ${C.border}`, display: "flex", alignItems: "center", gap: 11 }}>
        <div style={{ width: 34, height: 34, flex: "none", borderRadius: 8, background: tile, color, display: "grid", placeItems: "center" }}>
          <Icon name={icon} size={18} stroke={1.6} />
        </div>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontSize: 13.5, fontWeight: 600, color: "#fff", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{name}</div>
          <div style={{ fontSize: 11, color: C.muted2, fontFamily: MONO }}>{kind}</div>
        </div>
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: 16 }}>
        {trackKey && <TrackPanel trackKey={trackKey} dub={dub} transport={transport} trackCtl={trackCtl} />}

        {!sel && !isTrack && (
          <div style={{ textAlign: "center", padding: "36px 10px", color: C.muted3 }}>
            <svg viewBox="0 0 24 24" width={30} height={30} fill="none" stroke="currentColor" strokeWidth={1.5} style={{ marginBottom: 12 }}>
              <path d="M12 3 2 8.5 12 14l10-5.5z" />
              <path d="M2 8.5V16l10 5.5L22 16V8.5" />
            </svg>
            <div style={{ fontSize: 13, lineHeight: 1.5 }}>Chọn một clip trên timeline để chỉnh sửa,<br />hoặc bấm tên track để chỉnh cả track.</div>
          </div>
        )}

        {sel && (sel.type === "video" || sel.type === "image") && (
          <>
            <div style={SECTION}>Biến đổi</div>
            <Slider label="Tỉ lệ" min={20} max={200} value={sel.scale ?? 100} onChange={(v) => actions.setClipNum("scale", v)} display={`${sel.scale ?? 100}%`} />
            <Slider label="Dọc" min={-100} max={100} value={sel.posY ?? 0} onChange={(v) => actions.setClipNum("posY", v)} display={`${sel.posY ?? 0}`} mode="center" />
            <Slider label="Độ mờ" min={0} max={100} value={sel.opacity ?? 100} onChange={(v) => actions.setClipNum("opacity", v)} onCommit={isUserClip ? (v) => commitClip({ opacity: v / 100 }) : undefined} display={`${sel.opacity ?? 100}%`} />
            {sel.type === "video" && (
              <>
                <Divider />
                <div style={SECTION}>Âm lượng &amp; tốc độ</div>
                <Slider label="Âm lượng" min={0} max={100} value={sel.vol ?? 100} onChange={(v) => actions.setClipNum("vol", v)} onCommit={isUserClip ? (v) => commitClip({ volume: v / 100 }) : undefined} display={`${sel.vol ?? 100}`} />
                <Divider />
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 13 }}>
                  <span style={{ fontSize: 11, fontWeight: 600, letterSpacing: ".08em", textTransform: "uppercase", color: C.muted2 }}>Màu sắc</span>
                  <button onClick={actions.resetColor} style={{ border: "none", background: "transparent", color: C.coral, fontSize: 11, fontWeight: 600, cursor: "pointer" }}>Đặt lại</button>
                </div>
                <Slider label="Sáng" labelW={62} min={-100} max={100} value={sel.bri ?? 0} onChange={(v) => actions.setClipNum("bri", v)} display={`${sel.bri ?? 0}`} mode="center" />
                <Slider label="Tương phản" labelW={62} min={-100} max={100} value={sel.con ?? 0} onChange={(v) => actions.setClipNum("con", v)} display={`${sel.con ?? 0}`} mode="center" />
                <Slider label="Bão hoà" labelW={62} min={-100} max={100} value={sel.sat ?? 0} onChange={(v) => actions.setClipNum("sat", v)} display={`${sel.sat ?? 0}`} mode="center" />
              </>
            )}
          </>
        )}

        {sel && sel.type === "audio" && (
          <>
            <div style={SECTION}>Âm thanh</div>
            <Slider label="Âm lượng" labelW={60} min={0} max={100} value={sel.vol ?? 100} onChange={(v) => actions.setClipNum("vol", v)} onCommit={isUserClip ? (v) => commitClip({ volume: v / 100 }) : undefined} display={`${sel.vol ?? 100}`} />
            <Slider label="Tốc độ" labelW={60} min={50} max={200} value={sel.speed ?? 100} onChange={(v) => actions.setClipNum("speed", v)} display={`${((sel.speed ?? 100) / 100).toFixed(2)}`} />
            <Divider />
            <div style={SECTION}>Fade</div>
            <Slider label="Vào" labelW={60} min={0} max={100} value={sel.fadeIn ?? 0} onChange={(v) => actions.setClipNum("fadeIn", v)} display={`${(((sel.fadeIn ?? 0) / 100) * 2).toFixed(1)}s`} />
            <Slider label="Ra" labelW={60} min={0} max={100} value={sel.fadeOut ?? 0} onChange={(v) => actions.setClipNum("fadeOut", v)} display={`${(((sel.fadeOut ?? 0) / 100) * 2).toFixed(1)}s`} />
          </>
        )}

        {sel && sel.type === "sub" && (
          <>
            <div style={{ ...SECTION, marginBottom: 11 }}>{subSegId ? "Nội dung (tiếng Việt)" : "Nội dung (gốc)"}</div>
            <input
              value={sel.text ?? ""}
              readOnly={!subSegId}
              onChange={(e) => actions.setClipText(e.target.value)}
              onBlur={(e) => commitSub(e.target.value)}
              style={{ width: "100%", background: C.inset, border: `1px solid ${C.borderInset}`, borderRadius: 7, color: subSegId ? "#fff" : C.muted, fontFamily: FONT, fontSize: 13.5, padding: "9px 11px", outline: "none", marginBottom: 18 }}
            />
            {!subSegId && <div style={{ fontSize: 10.5, color: C.muted3, marginTop: -12, marginBottom: 16 }}>Phụ đề gốc chỉ để xem. Sửa bản dịch ở track “Phụ đề Việt”.</div>}
            <div style={{ ...SECTION, marginBottom: 11 }}>Kiểu chữ</div>
            <Slider label="Cỡ chữ" labelW={54} min={18} max={52} value={state.subStyle.size} onChange={(v) => actions.setSubNum("size", v)} onCommit={(v) => void dub.patchSettings({ sub_size: v })} display={`${state.subStyle.size}`} />
            <Slider label="Vị trí" labelW={54} min={20} max={92} value={state.subStyle.pos} onChange={(v) => actions.setSubNum("pos", v)} display={`${state.subStyle.pos}`} />
            <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 16 }}>
              <span style={{ width: 54, flex: "none", fontSize: 12, color: C.steel }}>Màu</span>
              <div style={{ display: "flex", gap: 7 }}>
                {SUBC.map((sc) => (
                  <button key={sc} onClick={() => { actions.setSubColor(sc); void dub.patchSettings({ sub_color: sc }); }} style={{ width: 24, height: 24, borderRadius: "50%", background: sc, border: `2px solid ${sc === state.subStyle.color ? "#fff" : "transparent"}`, cursor: "pointer" }} />
                ))}
              </div>
            </div>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 14 }}>
              <span style={{ fontSize: 12, color: C.steel }}>Nền chữ</span>
              <Toggle on={state.subStyle.bg} onClick={actions.toggleSubBg} />
            </div>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <div>
                <div style={{ fontSize: 12, color: C.steel }}>Song ngữ</div>
                <div style={{ fontSize: 10.5, color: C.muted3, marginTop: 1 }}>Hiện tiếng Trung trên tiếng Việt</div>
              </div>
              <Toggle on={state.subStyle.bilingual} onClick={() => { actions.toggleBil(); void dub.patchSettings({ sub_bilingual: !state.subStyle.bilingual }); }} />
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function TrackPanel({ trackKey, dub, transport, trackCtl }: { trackKey: string; dub: DubProjectHook; transport: Transport; trackCtl: TrackCtl }) {
  const ak = trackAudioKind(trackKey);
  const enabled = trackCtl.enabled(trackKey);

  if (ak === "original" || ak === "vn") {
    return (
      <>
        <div style={SECTION}>Âm lượng track</div>
        <TrackVolume kind={ak} value0={trackCtl.volume(trackKey) ?? 0} transport={transport} onCommit={(v) => trackCtl.setVolume(trackKey, v)} />
        <Divider />
        <RowToggle label="Bật track" sub={ak === "original" ? "Nghe + trộn tiếng gốc khi xuất" : "Nghe + trộn giọng lồng tiếng khi xuất"} on={enabled} onClick={() => trackCtl.toggle(trackKey)} />
      </>
    );
  }
  if (ak === "sub") {
    return (
      <>
        <div style={SECTION}>Phụ đề Việt</div>
        <RowToggle label="Ghi vào video" sub="Hiện ở preview & in cứng vào video khi xuất" on={enabled} onClick={() => trackCtl.toggle(trackKey)} />
        {!dub.detail?.project.vn_track_path && <div style={{ fontSize: 10.5, color: C.muted3, marginTop: 12 }}>Phụ đề lấy từ bản dịch của từng câu.</div>}
      </>
    );
  }
  if (ak === "subSrc") {
    return (
      <>
        <div style={SECTION}>Phụ đề gốc</div>
        <RowToggle label="Hiện phụ đề gốc" sub="Hiện song ngữ ở preview & khi xuất" on={enabled} onClick={() => trackCtl.toggle(trackKey)} />
      </>
    );
  }
  if (ak === "video") {
    return (
      <>
        <div style={SECTION}>Track video</div>
        <RowToggle label="Bật track video" sub="Tắt = xuất chỉ âm thanh (không hình)" on={enabled} onClick={() => trackCtl.toggle(trackKey)} />
        <div style={{ fontSize: 10.5, color: C.muted3, marginTop: 12, lineHeight: 1.5 }}>Kéo clip video ra sau để chừa khoảng trống đầu video (lead-in); khi xuất sẽ có đoạn đen ở đầu.</div>
      </>
    );
  }
  return <div style={{ fontSize: 13, color: C.muted3, lineHeight: 1.5 }}>Track này không có cấu hình âm lượng.</div>;
}

function TrackVolume({ kind, value0, transport, onCommit }: { kind: "original" | "vn"; value0: number; transport: Transport; onCommit: (v: number) => void }) {
  const [v, setV] = useState(Math.round(value0 * 100));
  useEffect(() => setV(Math.round(value0 * 100)), [value0]);
  const apply = (n: number) => (kind === "original" ? transport.setOrigVolume(n / 100) : transport.setVnVolume(n / 100));
  const fill = `linear-gradient(to right,${C.coral} ${v}%,${C.border} ${v}%)`;
  const commit = (e: React.PointerEvent<HTMLInputElement> | React.MouseEvent<HTMLInputElement>) =>
    onCommit(Number((e.currentTarget as HTMLInputElement).value) / 100);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 13 }}>
      <span style={{ width: 58, flex: "none", fontSize: 12, color: C.steel }}>Âm lượng</span>
      <input
        type="range"
        min={0}
        max={100}
        value={v}
        onChange={(e) => { const n = Number(e.target.value); setV(n); apply(n); }}
        onPointerUp={commit}
        onMouseUp={commit}
        style={{ flex: 1, background: fill }}
      />
      <span style={{ width: 42, textAlign: "right", color: "#fff", fontFamily: MONO, fontSize: 12 }}>{v}</span>
    </div>
  );
}

function RowToggle({ label, sub, on, onClick }: { label: string; sub: string; on: boolean; onClick: () => void }) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 10 }}>
      <div>
        <div style={{ fontSize: 12, color: C.steel }}>{label}</div>
        <div style={{ fontSize: 10.5, color: C.muted3, marginTop: 1 }}>{sub}</div>
      </div>
      <Toggle on={on} onClick={onClick} />
    </div>
  );
}
