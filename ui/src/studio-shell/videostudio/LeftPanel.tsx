import { C, FONT, MONO } from "../theme";
import { Icon } from "../icons";
import { HoverBox } from "../ui";
import { fmtBytes, fmtDuration } from "../../components/dubbing/shared";
import { ORDER, STEPS, type DubProjectHook } from "./useDubProject";
import type { StudioActions, StudioState } from "./useStudio";
import type { DubStep } from "../../studioApi";

interface Props {
  state: StudioState;
  actions: StudioActions;
  dub: DubProjectHook;
}

const tabBtn = (active: boolean): React.CSSProperties => ({
  flex: 1, border: "none", background: "transparent", padding: "12px 4px", cursor: "pointer",
  fontFamily: FONT, fontSize: 13, fontWeight: active ? 600 : 500, color: active ? "#fff" : C.muted4,
  boxShadow: active ? `inset 0 -2px 0 0 ${C.purple}` : "none",
  display: "flex", alignItems: "center", justifyContent: "center", gap: 6,
});

/** Reached pipeline index, resolving a busy status (…ing) to its last-done step. */
function doneIdx(status: string): number {
  if (ORDER.includes(status)) return ORDER.indexOf(status);
  const st = STEPS.find((s) => s.busy === status);
  return st ? ORDER.indexOf(st.from) : -1;
}

const STAGE_META: { step: DubStep; num: number; title: string; sub: string; runLabel: string }[] = [
  { step: "extract", num: 1, title: "Tách giọng", sub: "Tách lời thoại khỏi nhạc nền", runLabel: "Chạy" },
  { step: "analyze", num: 2, title: "Phân tích → Phụ đề gốc", sub: "Nhận dạng & tách câu thoại", runLabel: "Chạy" },
  { step: "translate", num: 3, title: "Dịch → Phụ đề tiếng Việt", sub: "Dịch theo timestamp", runLabel: "Dịch" },
  { step: "synthesize", num: 4, title: "Đọc TTS → Lồng tiếng Việt", sub: "Sinh giọng đọc tiếng Việt", runLabel: "Tạo giọng đọc" },
];

export function LeftPanel({ state, actions, dub }: Props) {
  const isMedia = state.tab === "media";
  return (
    <div style={{ width: 300, flex: "none", background: C.panel, borderRight: `1px solid ${C.border}`, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div style={{ flex: "none", display: "flex", borderBottom: `1px solid ${C.border}`, padding: "0 6px" }}>
        <button onClick={() => actions.setTab("media")} style={tabBtn(isMedia)}>Phương tiện</button>
        <button onClick={() => actions.setTab("dub")} style={tabBtn(!isMedia)}>
          Lồng tiếng <span style={{ width: 6, height: 6, borderRadius: "50%", background: C.coral, display: "inline-block" }} />
        </button>
      </div>
      {isMedia ? <MediaTab dub={dub} /> : <DubTab dub={dub} />}
    </div>
  );
}

function MediaTab({ dub }: { dub: DubProjectHook }) {
  const p = dub.detail?.project;
  const info = dub.info;
  const rows: [string, string][] = [
    ["Thời lượng", fmtDuration(info?.duration ?? null)],
    ["Dung lượng", fmtBytes(info?.size ?? null)],
    ["Định dạng", info?.format_name ?? "—"],
    ["Video", info?.video ? `${info.video.codec ?? "?"} · ${info.video.width ?? "?"}×${info.video.height ?? "?"}` : "—"],
    ["Âm thanh", info?.audio ? `${info.audio.codec ?? "?"} · ${info.audio.channels ?? "?"}ch` : "—"],
  ];
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <div style={{ fontSize: 11, fontWeight: 600, letterSpacing: ".07em", textTransform: "uppercase", color: C.muted2, margin: "0 2px 10px" }}>Nguồn</div>
      <div style={{ display: "flex", alignItems: "center", gap: 10, background: C.panel2, border: "1px solid #2d303e", borderRadius: 9, padding: 8, marginBottom: 14 }}>
        <div style={{ width: 40, height: 40, flex: "none", borderRadius: 6, background: "rgba(234,124,105,.16)", display: "grid", placeItems: "center", color: C.coral }}>
          <Icon name="film" size={20} stroke={1.6} />
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 12.5, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p?.name ?? "Đang tải…"}</div>
          <div style={{ fontSize: 10.5, color: C.muted2, fontFamily: MONO, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p?.video_path ?? ""}</div>
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {rows.map(([k, v]) => (
          <div key={k} style={{ display: "flex", justifyContent: "space-between", gap: 10, fontSize: 12 }}>
            <span style={{ color: C.muted }}>{k}</span>
            <span style={{ color: "#fff", fontFamily: MONO, fontSize: 11.5, textAlign: "right", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{v}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function DubTab({ dub }: { dub: DubProjectHook }) {
  const p = dub.detail?.project;
  if (!p) {
    return (
      <div style={{ flex: 1, overflowY: "auto", padding: "14px 12px" }}>
        <div style={{ textAlign: "center", padding: "48px 18px", color: C.muted3, fontSize: 13.5 }}>{dub.err ?? "Đang tải dự án…"}</div>
      </div>
    );
  }

  const status = p.status;
  const di = doneIdx(status);
  const working = dub.busy || dub.autoRun;
  // A step failed if the project carries an error: status was reverted to that
  // step's prerequisite, so the failed step is the current "next" one.
  const projErr = p.error?.trim() ? p.error : null;

  const curVoice = (() => {
    const sp = dub.detail?.speakers ?? [];
    if (!sp.length) return "";
    const first = sp[0].voice ?? "";
    return sp.every((s) => (s.voice ?? "") === first) ? first : "";
  })();

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "14px 12px" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, background: C.inset, border: `1px solid ${C.borderInset2}`, borderRadius: 9, padding: "9px 11px", marginBottom: 12 }}>
        <span style={{ width: 7, height: 7, borderRadius: "50%", background: C.coral, flex: "none" }} />
        <span style={{ fontSize: 11.5, color: C.muted, flex: "none" }}>Lồng tiếng cho:</span>
        <span style={{ fontSize: 12, fontWeight: 600, color: "#fff", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p.name}</span>
      </div>

      {dub.err && (
        <div style={{ marginBottom: 12, border: "1px solid rgba(255,124,163,.3)", background: "rgba(255,124,163,.1)", color: C.pink, borderRadius: 8, padding: "8px 11px", fontSize: 11.5 }}>{dub.err}</div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {STAGE_META.map((meta) => {
          const info = STEPS.find((s) => s.step === meta.step)!;
          const done = ORDER.indexOf(info.done) <= di;
          const running = status === info.busy;
          const isNext = info.from === status;
          const failed = isNext && !running && !!projErr;
          const locked = !done && !running && !isNext;
          const segs = dub.detail?.segments ?? [];
          const previewLines =
            meta.step === "analyze" && done
              ? segs.slice(0, 3).map((s) => ({ tc: Math.round(s.start_s) + "s", txt: s.text_src }))
              : meta.step === "translate" && done
                ? segs.slice(0, 3).map((s) => ({ tc: Math.round(s.start_s) + "s", txt: s.text_vi || "(chưa dịch)" }))
                : undefined;
          return (
            <StageCard
              key={meta.step}
              num={meta.num}
              title={meta.title}
              sub={done && meta.step === "extract" ? "→ Giọng gốc + Nhạc nền" : meta.sub}
              done={done}
              running={running}
              locked={locked}
              working={working}
              failed={failed}
              error={failed ? projErr : undefined}
              runLabel={failed ? "Thử lại" : done ? "Chạy lại" : meta.runLabel}
              onRun={() => dub.run(meta.step)}
              previewLines={previewLines}
              voice={meta.step === "synthesize" && !locked ? curVoice : undefined}
              voiceOpts={dub.voiceOpts}
              onVoice={(v) => void dub.setAllSpeakerVoice(v || null)}
              extra={
                meta.step === "translate" && done && dub.longCount > 0 ? (
                  <div style={{ marginTop: 10, display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8, border: "1px solid rgba(255,181,114,.3)", background: "rgba(255,181,114,.1)", borderRadius: 7, padding: "7px 9px" }}>
                    <span style={{ fontSize: 10.5, color: C.orange }}>{dub.longCount} câu quá dài</span>
                    <button onClick={() => void dub.reshorten()} disabled={working} style={{ border: "none", background: "rgba(255,181,114,.2)", color: C.orange, borderRadius: 6, padding: "4px 9px", fontSize: 11, fontWeight: 600, cursor: working ? "default" : "pointer", fontFamily: FONT }}>Dịch ngắn lại</button>
                  </div>
                ) : undefined
              }
            />
          );
        })}
      </div>
    </div>
  );
}

interface StageCardProps {
  num: number;
  title: string;
  sub: string;
  done: boolean;
  running: boolean;
  locked: boolean;
  working: boolean;
  failed?: boolean;
  error?: string | null;
  runLabel: string;
  onRun: () => void;
  previewLines?: { tc: string; txt: string }[];
  voice?: string;
  voiceOpts?: { value: string; label: string }[];
  onVoice?: (v: string) => void;
  extra?: React.ReactNode;
}

function StageCard({ num, title, sub, done, running, locked, working, failed, error, runLabel, onRun, previewLines, voice, voiceOpts, onVoice, extra }: StageCardProps) {
  const cardBorder = failed ? "rgba(255,124,163,.45)" : done ? "rgba(80,209,170,.35)" : running ? "rgba(255,181,114,.4)" : C.borderSoft;
  const cardBg = failed ? "rgba(255,124,163,.06)" : running ? "rgba(255,181,114,.06)" : C.panel2;
  const iconBg = failed ? "rgba(255,124,163,.2)" : done ? "rgba(80,209,170,.2)" : running ? "rgba(255,181,114,.2)" : C.panel3;
  const iconFg = failed ? C.pink : done ? C.teal : running ? C.orange : C.muted;
  const runDisabled = locked || running || working;

  return (
    <div style={{ border: `1px solid ${cardBorder}`, background: cardBg, borderRadius: 11, padding: 13, opacity: locked ? 0.5 : 1 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ width: 26, height: 26, flex: "none", borderRadius: 7, background: iconBg, color: iconFg, display: "grid", placeItems: "center", fontFamily: MONO, fontSize: 12, fontWeight: 700 }}>
          {failed ? "!" : done ? <Icon name="check" size={14} stroke={3} /> : running ? <span style={{ width: 12, height: 12, border: "2px solid currentColor", borderTopColor: "transparent", borderRadius: "50%", animation: "bss-spin .7s linear infinite" }} /> : num}
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: locked ? C.muted : "#fff" }}>{title}</div>
          <div style={{ fontSize: 10.5, color: C.muted4 }}>{sub}</div>
        </div>
        {locked && <Icon name="lock" size={14} stroke={1.8} color={C.muted5} />}
      </div>

      {failed && error && (
        <div style={{ marginTop: 10, border: "1px solid rgba(255,124,163,.3)", background: "rgba(255,124,163,.1)", color: C.pink, borderRadius: 7, padding: "7px 9px", fontSize: 11, lineHeight: 1.45, maxHeight: 140, overflow: "auto", whiteSpace: "pre-wrap", fontFamily: MONO }}>
          {error}
        </div>
      )}

      {voice !== undefined && (
        <div style={{ position: "relative", marginTop: 11 }}>
          <select
            value={voice}
            onChange={(e) => onVoice?.(e.target.value)}
            style={{ width: "100%", appearance: "none", WebkitAppearance: "none", background: C.inset, border: `1px solid ${C.borderInset}`, borderRadius: 7, color: "#fff", fontSize: 12.5, padding: "8px 30px 8px 11px", cursor: "pointer", outline: "none", fontFamily: FONT }}
          >
            <option value="">(tự động theo giới tính)</option>
            {(voiceOpts ?? []).map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
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

      {extra}

      {!locked && (
        <div style={{ marginTop: 11 }}>
          <HoverBox
            as="button"
            onClick={runDisabled ? undefined : onRun}
            title={done && !failed ? "Chạy lại bước này (ghi đè kết quả cũ)" : undefined}
            style={{
              width: "100%", height: 32, borderRadius: 7, fontFamily: FONT, fontSize: 12.5, fontWeight: 600,
              display: "flex", alignItems: "center", justifyContent: "center", gap: 6,
              cursor: runDisabled ? "default" : "pointer",
              transition: "transform .08s, background .12s, filter .12s",
              border: `1px solid ${
                running ? "rgba(255,181,114,.4)" : failed ? "rgba(255,124,163,.5)" : done ? C.borderInset2 : "rgba(146,136,224,.45)"
              }`,
              background: running ? "rgba(255,181,114,.12)" : failed ? "rgba(255,124,163,.14)" : done ? "transparent" : "rgba(146,136,224,.14)",
              color: running ? C.orange : failed ? C.pink : done ? C.muted3 : C.purpleLt,
            }}
            hoverStyle={runDisabled ? undefined : { background: failed ? "rgba(255,124,163,.24)" : done ? "rgba(146,136,224,.1)" : "rgba(146,136,224,.24)", filter: "brightness(1.08)" }}
            activeStyle={runDisabled ? undefined : { transform: "scale(.97)", filter: "brightness(.95)" }}
          >
            {running ? (
              <>
                <span style={{ width: 12, height: 12, border: "2px solid currentColor", borderTopColor: "transparent", borderRadius: "50%", animation: "bss-spin .7s linear infinite" }} />
                Đang xử lý…
              </>
            ) : (
              <>
                {failed && <Icon name="undo" size={13} stroke={2} />}
                {runLabel}
              </>
            )}
          </HoverBox>
        </div>
      )}
    </div>
  );
}
