import { C, FONT, MONO } from "../theme";
import { Icon, type IconName } from "../icons";
import { HoverBox } from "../ui";
import { DUB, fmt } from "./constants";
import type { StudioActions, StudioState } from "./useStudio";
import type { Clip, PipeKey, PipeStatus } from "./types";

interface Props {
  state: StudioState;
  actions: StudioActions;
}

const tabBtn = (active: boolean): React.CSSProperties => ({
  flex: 1, border: "none", background: "transparent", padding: "12px 4px", cursor: "pointer",
  fontFamily: FONT, fontSize: 13, fontWeight: active ? 600 : 500, color: active ? "#fff" : C.muted4,
  boxShadow: active ? `inset 0 -2px 0 0 ${C.purple}` : "none",
  display: "flex", alignItems: "center", justifyContent: "center", gap: 6,
});

const LIBRARY: { name: string; meta: string; tile: string; color: string; icon: IconName; add: keyof StudioActions }[] = [
  { name: "养老_1080p.mp4", meta: "00:11 · MP4", tile: "rgba(234,124,105,.16)", color: C.coral, icon: "film", add: "addVideo" },
  { name: "logo.png", meta: "PNG · 512²", tile: "rgba(101,176,246,.16)", color: C.blue, icon: "image", add: "addImage" },
  { name: "nhac_nen.mp3", meta: "00:11 · MP3", tile: "rgba(80,209,170,.16)", color: C.teal, icon: "music", add: "addMusic" },
];

export function LeftPanel({ state, actions }: Props) {
  const isMedia = state.tab === "media";

  return (
    <div style={{ width: 300, flex: "none", background: C.panel, borderRight: `1px solid ${C.border}`, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div style={{ flex: "none", display: "flex", borderBottom: `1px solid ${C.border}`, padding: "0 6px" }}>
        <button onClick={() => actions.setTab("media")} style={tabBtn(isMedia)}>Phương tiện</button>
        <button onClick={() => actions.setTab("dub")} style={tabBtn(!isMedia)}>
          Lồng tiếng <span style={{ width: 6, height: 6, borderRadius: "50%", background: C.coral, display: "inline-block" }} />
        </button>
      </div>
      {isMedia ? <MediaTab actions={actions} /> : <DubTab state={state} actions={actions} />}
    </div>
  );
}

function MediaTab({ actions }: { actions: StudioActions }) {
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <HoverBox
        style={{ border: "1.5px dashed #3d4051", borderRadius: 9, padding: "15px 12px", display: "flex", alignItems: "center", gap: 11, cursor: "pointer", marginBottom: 14 }}
        hoverStyle={{ borderColor: C.purple, background: "rgba(146,136,224,.06)" }}
      >
        <div style={{ width: 34, height: 34, flex: "none", borderRadius: 8, background: C.panel3, display: "grid", placeItems: "center", color: C.purple }}>
          <Icon name="plus" size={18} stroke={2} />
        </div>
        <div>
          <div style={{ fontSize: 12.5, fontWeight: 600 }}>Nhập phương tiện</div>
          <div style={{ fontSize: 10.5, color: C.muted2 }}>Video · Âm thanh · Hình ảnh</div>
        </div>
      </HoverBox>
      <div style={{ fontSize: 11, fontWeight: 600, letterSpacing: ".07em", textTransform: "uppercase", color: C.muted2, margin: "0 2px 10px" }}>Trong dự án</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {LIBRARY.map((m) => (
          <HoverBox key={m.name} style={{ display: "flex", alignItems: "center", gap: 10, background: C.panel2, border: "1px solid #2d303e", borderRadius: 9, padding: 8, cursor: "pointer" }} hoverStyle={{ borderColor: "#4a4e5e" }}>
            <div style={{ width: 40, height: 40, flex: "none", borderRadius: 6, background: m.tile, display: "grid", placeItems: "center", color: m.color, overflow: "hidden" }}>
              <Icon name={m.icon} size={20} stroke={1.6} />
            </div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 12.5, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{m.name}</div>
              <div style={{ fontSize: 10.5, color: C.muted2, fontFamily: MONO }}>{m.meta}</div>
            </div>
            <HoverBox
              as="button"
              onClick={() => (actions[m.add] as () => void)()}
              title="Thêm vào timeline"
              style={{ width: 26, height: 26, flex: "none", border: "none", background: C.panel3, color: C.steel, borderRadius: 6, display: "grid", placeItems: "center", cursor: "pointer" }}
              hoverStyle={{ background: C.purple, color: "#fff" }}
            >
              <Icon name="plus" size={15} stroke={2} />
            </HoverBox>
          </HoverBox>
        ))}
      </div>
    </div>
  );
}

interface Stage {
  key: PipeKey;
  num: number;
  title: string;
  sub: string;
  done: boolean;
  running: boolean;
  locked: boolean;
}

function DubTab({ state, actions }: { state: StudioState; actions: StudioActions }) {
  const sel = state.clips.find((c: Clip) => c.id === state.sel);
  const dubVidId = sel && sel.type === "video" ? sel.id : null;

  if (!dubVidId) {
    return (
      <div style={{ flex: 1, overflowY: "auto", padding: "14px 12px" }}>
        <div style={{ textAlign: "center", padding: "48px 18px", color: C.muted3 }}>
          <div style={{ width: 46, height: 46, margin: "0 auto 14px", borderRadius: 11, background: C.panel2, display: "grid", placeItems: "center", color: C.faint }}>
            <Icon name="film" size={22} stroke={1.6} />
          </div>
          <div style={{ fontSize: 13.5, color: C.ink2, lineHeight: 1.5 }}>
            Chọn một <b style={{ color: "#fff" }}>video</b> trên timeline<br />để lồng tiếng cho video đó.
          </div>
        </div>
      </div>
    );
  }

  const dstate = actions.getDub(dubVidId);
  const P = dstate.pipe;
  const dins = dstate.inserted;
  const done = (k: PipeKey) => P[k] === "done";

  const meta = (key: PipeKey, num: number, title: string, sub: string, prevDone: boolean): Stage => {
    const st: PipeStatus = P[key];
    return { key, num, title, sub, done: st === "done", running: st === "running", locked: st === "idle" && !prevDone };
  };
  const stTach = meta("tach", 1, "Tách giọng", done("tach") ? "→ Giọng gốc + Nhạc nền" : "Tách lời thoại khỏi nhạc nền", true);
  const stPhan = meta("phan", 2, "Phân tích → Phụ đề gốc", "Nhận dạng & tách câu thoại", done("tach"));
  const stDich = meta("dich", 3, "Dịch → Phụ đề tiếng Việt", "Dịch theo timestamp", done("phan"));
  const stTts = meta("tts", 4, "Đọc TTS → Lồng tiếng Việt", "Sinh giọng đọc tiếng Việt", done("dich"));

  const preview3 = (key: "zh" | "vi") => DUB.slice(0, 3).map((l) => ({ tc: fmt(l.t).slice(3, 5) + "s", txt: key === "zh" ? l.zh : l.vi }));

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "14px 12px" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, background: C.inset, border: `1px solid ${C.borderInset2}`, borderRadius: 9, padding: "9px 11px", marginBottom: 12 }}>
        <span style={{ width: 7, height: 7, borderRadius: "50%", background: C.coral, flex: "none" }} />
        <span style={{ fontSize: 11.5, color: C.muted, flex: "none" }}>Đang lồng tiếng:</span>
        <span style={{ fontSize: 12, fontWeight: 600, color: "#fff", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{sel?.name}</span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <StageCard stage={stTach} runLabel="Chạy" onRun={() => actions.run("tach")} />
        <StageCard stage={stPhan} runLabel="Chạy" onRun={() => actions.run("phan")} previewLines={done("phan") ? preview3("zh") : undefined} insert={{ ready: done("phan"), done: dins.szh, label: "Chèn phụ đề gốc", onInsert: () => actions.insertSubs(dubVidId, "szh") }} />
        <StageCard stage={stDich} runLabel="Dịch" onRun={() => actions.run("dich")} previewLines={done("dich") ? preview3("vi") : undefined} insert={{ ready: done("dich"), done: dins.svi, label: "Chèn phụ đề Việt", onInsert: () => actions.insertSubs(dubVidId, "svi") }} />
        <StageCard stage={stTts} runLabel="Tạo giọng đọc" onRun={() => actions.run("tts")} voice={dstate.voice} onVoice={actions.setVoice} insert={{ ready: done("tts"), done: dins.tts, label: "Chèn audio lồng tiếng", onInsert: () => actions.insertTts(dubVidId) }} />
      </div>
    </div>
  );
}

interface InsertCfg {
  ready: boolean;
  done: boolean;
  label: string;
  onInsert: () => void;
}

function StageCard({ stage, runLabel, onRun, previewLines, insert, voice, onVoice }: {
  stage: Stage;
  runLabel: string;
  onRun: () => void;
  previewLines?: { tc: string; txt: string }[];
  insert?: InsertCfg;
  voice?: string;
  onVoice?: (v: string) => void;
}) {
  const { done, running, locked } = stage;
  const cardBorder = done ? "rgba(80,209,170,.35)" : running ? "rgba(255,181,114,.4)" : C.borderSoft;
  const cardBg = running ? "rgba(255,181,114,.06)" : C.panel2;
  const iconBg = done ? "rgba(80,209,170,.2)" : running ? "rgba(255,181,114,.2)" : C.panel3;
  const iconFg = done ? C.teal : running ? C.orange : C.muted;

  return (
    <div style={{ border: `1px solid ${cardBorder}`, background: cardBg, borderRadius: 11, padding: 13, opacity: locked ? 0.5 : 1 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ width: 26, height: 26, flex: "none", borderRadius: 7, background: iconBg, color: iconFg, display: "grid", placeItems: "center", fontFamily: MONO, fontSize: 12, fontWeight: 700 }}>
          {done ? <Icon name="check" size={14} stroke={3} /> : running ? <span style={{ width: 12, height: 12, border: "2px solid currentColor", borderTopColor: "transparent", borderRadius: "50%", animation: "bss-spin .7s linear infinite" }} /> : stage.num}
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: locked ? C.muted : "#fff" }}>{stage.title}</div>
          <div style={{ fontSize: 10.5, color: C.muted4 }}>{stage.sub}</div>
        </div>
        {locked && <Icon name="lock" size={14} stroke={1.8} color={C.muted5} />}
      </div>

      {voice !== undefined && !locked && (
        <div style={{ position: "relative", marginTop: 11 }}>
          <select
            value={voice}
            onChange={(e) => onVoice?.(e.target.value)}
            style={{ width: "100%", appearance: "none", WebkitAppearance: "none", background: C.inset, border: `1px solid ${C.borderInset}`, borderRadius: 7, color: "#fff", fontSize: 12.5, padding: "8px 30px 8px 11px", cursor: "pointer", outline: "none", fontFamily: FONT }}
          >
            <option>Mỹ Duyên — nữ, miền Nam</option>
            <option>Lan Anh — nữ, miền Bắc</option>
            <option>Minh Quân — nam, miền Bắc</option>
          </select>
          <Icon name="chevronD" size={14} stroke={2} color={C.muted3} style={{ position: "absolute", right: 10, top: "50%", transform: "translateY(-50%)", pointerEvents: "none" }} />
        </div>
      )}

      {previewLines && (
        <div style={{ marginTop: 11, background: C.insetDark, border: `1px solid ${C.borderInset2}`, borderRadius: 8, padding: "9px 11px", maxHeight: 84, overflow: "hidden" }}>
          {previewLines.map((ln, i) => (
            <div key={i} style={{ fontSize: 11.5, color: C.ink3, lineHeight: 1.5, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              <span style={{ color: C.muted3, fontFamily: MONO, fontSize: 10 }}>{ln.tc}</span> {ln.txt}
            </div>
          ))}
        </div>
      )}

      <div style={{ marginTop: 11, display: "flex", gap: 8 }}>
        {!done && (
          <button
            onClick={locked || running ? undefined : onRun}
            style={{
              flex: 1, height: 32, borderRadius: 7, fontFamily: FONT, fontSize: 12.5, fontWeight: 600,
              display: "flex", alignItems: "center", justifyContent: "center", gap: 6,
              cursor: locked || running ? "default" : "pointer",
              border: `1px solid ${locked ? C.borderInset2 : running ? "rgba(255,181,114,.4)" : "rgba(146,136,224,.45)"}`,
              background: locked ? C.inset : running ? "rgba(255,181,114,.12)" : "rgba(146,136,224,.14)",
              color: locked ? C.muted5 : running ? C.orange : C.purpleLt,
            }}
          >
            {running ? "Đang xử lý…" : locked ? "Khoá" : runLabel}
          </button>
        )}
        {insert?.ready && (
          <button
            onClick={insert.done ? undefined : insert.onInsert}
            style={{ flex: 1, height: 32, border: "none", borderRadius: 7, fontFamily: FONT, fontSize: 12.5, fontWeight: 600, display: "flex", alignItems: "center", justifyContent: "center", gap: 6, cursor: insert.done ? "default" : "pointer", background: insert.done ? "rgba(80,209,170,.16)" : C.purple, color: insert.done ? C.teal : "#fff" }}
          >
            {insert.done ? "✓ Đã chèn" : insert.label}
          </button>
        )}
      </div>
    </div>
  );
}
