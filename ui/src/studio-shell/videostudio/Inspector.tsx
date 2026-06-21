import { C, FONT, MONO } from "../theme";
import { Icon, type IconName } from "../icons";
import { SUBC } from "./constants";
import type { StudioActions, StudioState } from "./useStudio";
import type { Clip } from "./types";

interface Props {
  state: StudioState;
  actions: StudioActions;
}

const SECTION: React.CSSProperties = { fontSize: 11, fontWeight: 600, letterSpacing: ".08em", textTransform: "uppercase", color: C.muted2, marginBottom: 13 };
const cl = (v: number, mn: number, mx: number) => (((v - mn) / (mx - mn)) * 100).toFixed(1);

function Slider({ label, labelW = 58, min, max, value, onChange, display, mode = "left" }: {
  label: string;
  labelW?: number;
  min: number;
  max: number;
  value: number;
  onChange: (v: number) => void;
  display: string;
  mode?: "left" | "center";
}) {
  const fill =
    mode === "center"
      ? `linear-gradient(to right,${C.border} 50%,${C.coral} 50%)`
      : `linear-gradient(to right,${C.coral} ${cl(value, min, max)}%,${C.border} ${cl(value, min, max)}%)`;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 13 }}>
      <span style={{ width: labelW, flex: "none", fontSize: 12, color: C.steel }}>{label}</span>
      <input type="range" min={min} max={max} value={value} onChange={(e) => onChange(Number(e.target.value))} style={{ flex: 1, background: fill }} />
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

export function Inspector({ state, actions }: Props) {
  const sel = state.clips.find((c: Clip) => c.id === state.sel) ?? null;

  let name = "—";
  let kind = "";
  let tile: string = C.panel3;
  let color: string = C.muted;
  let icon: IconName = "film";
  if (sel) {
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
        {!sel && (
          <div style={{ textAlign: "center", padding: "36px 10px", color: C.muted3 }}>
            <svg viewBox="0 0 24 24" width={30} height={30} fill="none" stroke="currentColor" strokeWidth={1.5} style={{ marginBottom: 12 }}>
              <path d="M12 3 2 8.5 12 14l10-5.5z" />
              <path d="M2 8.5V16l10 5.5L22 16V8.5" />
            </svg>
            <div style={{ fontSize: 13, lineHeight: 1.5 }}>Chọn một clip trên timeline<br />để chỉnh sửa thuộc tính.</div>
          </div>
        )}

        {sel && (sel.type === "video" || sel.type === "image") && (
          <>
            <div style={SECTION}>Biến đổi</div>
            <Slider label="Tỉ lệ" min={20} max={200} value={sel.scale ?? 100} onChange={(v) => actions.setClipNum("scale", v)} display={`${sel.scale ?? 100}%`} />
            <Slider label="Dọc" min={-100} max={100} value={sel.posY ?? 0} onChange={(v) => actions.setClipNum("posY", v)} display={`${sel.posY ?? 0}`} mode="center" />
            <Slider label="Độ mờ" min={0} max={100} value={sel.opacity ?? 100} onChange={(v) => actions.setClipNum("opacity", v)} display={`${sel.opacity ?? 100}%`} />
            {sel.type === "video" && (
              <>
                <Divider />
                <div style={SECTION}>Âm lượng &amp; tốc độ</div>
                <Slider label="Âm lượng" min={0} max={100} value={sel.vol ?? 100} onChange={(v) => actions.setClipNum("vol", v)} display={`${sel.vol ?? 100}`} />
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
            <Slider label="Âm lượng" labelW={60} min={0} max={100} value={sel.vol ?? 100} onChange={(v) => actions.setClipNum("vol", v)} display={`${sel.vol ?? 100}`} />
            <Slider label="Tốc độ" labelW={60} min={50} max={200} value={sel.speed ?? 100} onChange={(v) => actions.setClipNum("speed", v)} display={`${((sel.speed ?? 100) / 100).toFixed(2)}`} />
            <Divider />
            <div style={SECTION}>Fade</div>
            <Slider label="Vào" labelW={60} min={0} max={100} value={sel.fadeIn ?? 0} onChange={(v) => actions.setClipNum("fadeIn", v)} display={`${(((sel.fadeIn ?? 0) / 100) * 2).toFixed(1)}s`} />
            <Slider label="Ra" labelW={60} min={0} max={100} value={sel.fadeOut ?? 0} onChange={(v) => actions.setClipNum("fadeOut", v)} display={`${(((sel.fadeOut ?? 0) / 100) * 2).toFixed(1)}s`} />
          </>
        )}

        {sel && sel.type === "sub" && (
          <>
            <div style={{ ...SECTION, marginBottom: 11 }}>Nội dung</div>
            <input
              value={sel.text ?? ""}
              onChange={(e) => actions.setClipText(e.target.value)}
              style={{ width: "100%", background: C.inset, border: `1px solid ${C.borderInset}`, borderRadius: 7, color: "#fff", fontFamily: FONT, fontSize: 13.5, padding: "9px 11px", outline: "none", marginBottom: 18 }}
            />
            <div style={{ ...SECTION, marginBottom: 11 }}>Kiểu chữ</div>
            <Slider label="Cỡ chữ" labelW={54} min={18} max={52} value={state.subStyle.size} onChange={(v) => actions.setSubNum("size", v)} display={`${state.subStyle.size}`} />
            <Slider label="Vị trí" labelW={54} min={20} max={92} value={state.subStyle.pos} onChange={(v) => actions.setSubNum("pos", v)} display={`${state.subStyle.pos}`} />
            <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 16 }}>
              <span style={{ width: 54, flex: "none", fontSize: 12, color: C.steel }}>Màu</span>
              <div style={{ display: "flex", gap: 7 }}>
                {SUBC.map((sc) => (
                  <button key={sc} onClick={() => actions.setSubColor(sc)} style={{ width: 24, height: 24, borderRadius: "50%", background: sc, border: `2px solid ${sc === state.subStyle.color ? "#fff" : "transparent"}`, cursor: "pointer" }} />
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
              <Toggle on={state.subStyle.bilingual} onClick={actions.toggleBil} />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
