import { useCallback, useEffect, useRef, useState } from "react";
import { getInfo, getVoices, type ServerInfo, type Voice } from "../api";
import { buildSubmit, useQueue, type QueueItem } from "../queue";
import { copyFile, isTauri, saveAsDialog } from "../platform";
import { useTtsSettings } from "../ttsSettings";
import { C, FONT, MONO, injectStudioStyles } from "../studio-shell/theme";

type Status = "starting" | "ready" | "error";

const SAMPLE =
  "[cười] Trời ơi, cái giọng nó tự nhiên mà mượt mà dã man, nghe không khác gì người thật luôn.";

// ─── Avatar colour cycle ──────────────────────────────────────────────────────
const AVATAR_BG = [
  "rgba(146,136,224,.2)",
  "rgba(234,124,105,.2)",
  "rgba(59,130,246,.2)",
  "rgba(80,209,170,.2)",
  "rgba(251,146,60,.2)",
];
const AVATAR_FG = ["#9288E0", "#EA7C69", "#60a5fa", "#50D1AA", "#fb923c"];

function avatarBg(i: number) { return AVATAR_BG[i % 5]; }
function avatarFg(i: number) { return AVATAR_FG[i % 5]; }

// ─── Helpers ──────────────────────────────────────────────────────────────────
function fmtTime(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

function estDur(len: number): string {
  return fmtTime(Math.max(0, Math.floor(len / 14)));
}

// ─── Main page ────────────────────────────────────────────────────────────────
export function StudioPage() {
  injectStudioStyles();

  const { outputDir, concurrency } = useTtsSettings();
  const [status, setStatus] = useState<Status>("starting");
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
  const voicesLoaded = useRef(false);

  const [text, setText] = useState(SAMPLE);
  const [voice, setVoice] = useState("");
  const [emotion, setEmotion] = useState("natural");
  const [temperature, setTemperature] = useState(0.8);
  const [topK, setTopK] = useState(25);
  const [topP, setTopP] = useState(0.95);
  const [repPen, setRepPen] = useState(1.2);
  const [format, setFormat] = useState<"wav" | "mp3">("mp3");

  // New local state
  const [voiceSearch, setVoiceSearch] = useState("");
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [volume, setVolume] = useState(1);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);

  const queue = useQueue(outputDir, concurrency);

  // Poll server health; load voices exactly once
  useEffect(() => {
    let alive = true;
    const tick = async () => {
      try {
        const i = await getInfo();
        if (!alive) return;
        setInfo(i);
        setStatus("ready");
        if (!voicesLoaded.current) {
          voicesLoaded.current = true;
          const v = await getVoices();
          if (!alive) return;
          setVoices(v);
          setVoice((prev) => prev || v[0]?.id || "");
        }
      } catch {
        if (alive) setStatus((s) => (s === "ready" ? "error" : "starting"));
      }
    };
    tick();
    const h = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(h);
    };
  }, []);

  // Audio player event wiring
  useEffect(() => {
    const el = audioRef.current;
    if (!el) return;
    const onTime = () => setCurrentTime(el.currentTime);
    const onDur = () => setDuration(el.duration || 0);
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onEnded = () => setPlaying(false);
    el.addEventListener("timeupdate", onTime);
    el.addEventListener("durationchange", onDur);
    el.addEventListener("play", onPlay);
    el.addEventListener("pause", onPause);
    el.addEventListener("ended", onEnded);
    return () => {
      el.removeEventListener("timeupdate", onTime);
      el.removeEventListener("durationchange", onDur);
      el.removeEventListener("play", onPlay);
      el.removeEventListener("pause", onPause);
      el.removeEventListener("ended", onEnded);
    };
  }, []);

  // When selected item changes to a done item, load & play
  useEffect(() => {
    const item = queue.items.find((x) => x.id === selectedItemId);
    if (item?.status === "done" && item.url && audioRef.current) {
      audioRef.current.src = item.url;
      audioRef.current.play().catch(() => {});
    }
  }, [selectedItemId, queue.items]);

  const selectedVoice = voices.find((v) => v.id === voice);
  const voiceLabel = selectedVoice?.label ?? voice;
  const selectedItem = queue.items.find((x) => x.id === selectedItemId) ?? null;
  const selectedVoiceIdx = voices.findIndex((v) => v.id === voice);

  const filteredVoices = voiceSearch.trim()
    ? voices.filter((v) => v.label.toLowerCase().includes(voiceSearch.trim().toLowerCase()))
    : voices;

  // Insert label at caret
  function insertLabel(label: string) {
    const el = textareaRef.current;
    if (!el) {
      setText((t) => `${t}[${label}] `);
      return;
    }
    const start = el.selectionStart;
    const end = el.selectionEnd;
    const ins = `[${label}] `;
    const newText = text.slice(0, start) + ins + text.slice(end);
    setText(newText);
    setTimeout(() => {
      el.selectionStart = el.selectionEnd = start + ins.length;
      el.focus();
    }, 0);
  }

  const generate = useCallback(() => {
    if (!text.trim()) return;
    const params = {
      text,
      voice: voice || undefined,
      emotion,
      temperature,
      top_k: topK,
      top_p: topP,
      repetition_penalty: repPen,
      format,
    };
    queue.enqueue(buildSubmit(params, voiceLabel));
  }, [text, voice, emotion, temperature, topK, topP, repPen, format, voiceLabel, queue]);

  const saveAs = useCallback(async (it: QueueItem) => {
    const fname = `beesoft.${it.format}`;
    if (isTauri() && it.serverPath) {
      const dest = await saveAsDialog(fname);
      if (dest) await copyFile(it.serverPath, dest);
    } else if (it.url) {
      const a = document.createElement("a");
      a.href = it.url;
      a.download = fname;
      a.click();
    }
  }, []);

  const togglePlayPause = () => {
    const el = audioRef.current;
    if (!el) return;
    if (playing) el.pause();
    else el.play().catch(() => {});
  };

  const seekTo = (pct: number) => {
    const el = audioRef.current;
    if (!el || !duration) return;
    el.currentTime = (pct / 100) * duration;
  };

  const setVolumeVal = (v: number) => {
    setVolume(v);
    if (audioRef.current) audioRef.current.volume = v;
  };

  const hasSelection = selectedItem?.status === "done";

  // ─── Styles (inline) ───────────────────────────────────────────────────────
  const $ = {
    root: {
      height: "100%",
      display: "flex",
      flexDirection: "column" as const,
      overflow: "hidden",
      fontFamily: FONT,
      background: C.appBg,
      color: C.ink,
    },

    // TOP BAR
    topBar: {
      height: 48,
      minHeight: 48,
      display: "flex",
      alignItems: "center",
      gap: 12,
      padding: "0 16px",
      background: C.panel,
      borderBottom: `1px solid ${C.border}`,
      flexShrink: 0,
    },
    speakerTile: {
      width: 32,
      height: 32,
      borderRadius: 8,
      background: "rgba(80,209,170,.16)",
      color: C.teal,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      fontSize: 16,
      flexShrink: 0,
    },
    topBarTitle: {
      fontSize: 14.5,
      fontWeight: 600,
      color: C.ink,
      lineHeight: 1.2,
    },
    topBarSub: {
      fontFamily: MONO,
      fontSize: 11,
      color: C.muted,
      lineHeight: 1.3,
    },
    topBarSpacer: { flex: 1 },
    versionBadge: {
      padding: "2px 8px",
      fontSize: 11,
      background: C.panel2,
      border: `1px solid ${C.border}`,
      borderRadius: 5,
      color: C.muted,
      cursor: "default",
      flexShrink: 0,
    },

    // MAIN ROW
    mainRow: {
      flex: 1,
      display: "flex",
      overflow: "hidden",
    },

    // LEFT PANEL
    leftPanel: {
      width: 264,
      minWidth: 264,
      display: "flex",
      flexDirection: "column" as const,
      background: C.panel,
      borderRight: `1px solid ${C.border}`,
      overflow: "hidden",
    },
    searchBox: {
      margin: "10px 10px 6px",
      padding: "6px 10px",
      background: C.panel2,
      border: `1px solid ${C.border}`,
      borderRadius: 8,
      color: "#ccc",
      fontSize: 11,
      outline: "none",
      width: "calc(100% - 20px)",
      boxSizing: "border-box" as const,
    },
    sectionHeader: {
      fontSize: 12,
      textTransform: "uppercase" as const,
      letterSpacing: "0.08em",
      color: C.muted3,
      padding: "4px 12px 6px",
    },
    voiceList: {
      flex: 1,
      overflowY: "auto" as const,
      padding: "0 6px",
    },
    voiceRow: (selected: boolean) => ({
      display: "flex",
      alignItems: "center",
      gap: 10,
      padding: "5px 6px",
      borderRadius: 8,
      cursor: "pointer",
      height: 44,
      border: selected ? "1px solid rgba(234,124,105,.5)" : "1px solid transparent",
      background: selected ? "rgba(234,124,105,.1)" : "transparent",
      transition: "background .12s",
    }),
    voiceAvatar: (idx: number) => ({
      width: 34,
      height: 34,
      borderRadius: "50%",
      background: avatarBg(idx < 0 ? 0 : idx),
      color: avatarFg(idx < 0 ? 0 : idx),
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      fontSize: 14,
      fontWeight: 600,
      flexShrink: 0,
    }),
    voiceName: {
      flex: 1,
      minWidth: 0,
    },
    voiceNameText: {
      fontSize: 14,
      color: "#d4d4d4",
      whiteSpace: "nowrap" as const,
      overflow: "hidden",
      textOverflow: "ellipsis",
    },
    voiceDesc: {
      fontSize: 10,
      color: C.muted3,
      marginTop: 1,
    },
    voicePreviewBtn: {
      width: 22,
      height: 22,
      borderRadius: "50%",
      background: C.panel2,
      border: "none",
      color: "#ccc",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      fontSize: 9,
      opacity: 0.3,
      cursor: "not-allowed",
      flexShrink: 0,
      padding: 0,
    },
    cloneSection: {
      borderTop: `1px solid ${C.borderSoft}`,
      padding: "8px 10px 10px",
    },
    cloneSectionHdr: {
      fontSize: 11,
      color: C.muted3,
      marginBottom: 6,
    },
    cloneRow: {
      display: "flex",
      gap: 6,
    },
    cloneBtn: {
      flex: 1,
      padding: "5px 0",
      background: C.panel2,
      border: `1px solid ${C.border}`,
      borderRadius: 6,
      color: C.muted,
      fontSize: 11,
      cursor: "not-allowed",
      opacity: 0.5,
    },

    // CENTER PANEL
    centerPanel: {
      flex: 1,
      display: "flex",
      flexDirection: "column" as const,
      background: C.content,
      overflow: "hidden",
    },
    editorArea: {
      flex: "0 0 62%",
      display: "flex",
      flexDirection: "column" as const,
      padding: "14px 16px 10px",
      minHeight: 0,
    },
    editorHeader: {
      display: "flex",
      alignItems: "center",
      marginBottom: 8,
    },
    editorLabel: {
      fontSize: 12,
      textTransform: "uppercase" as const,
      letterSpacing: "0.08em",
      color: C.muted3,
    },
    editorMeta: {
      marginLeft: "auto",
      fontSize: 11,
      color: C.muted3,
    },
    textarea: {
      flex: 1,
      background: C.inset,
      border: `1px solid ${C.borderInset}`,
      borderRadius: 11,
      color: "#d4d4d4",
      fontSize: 16,
      padding: "14px 16px",
      resize: "none" as const,
      lineHeight: 1.7,
      outline: "none",
      fontFamily: FONT,
      minHeight: 0,
      transition: "border-color .15s",
    },
    chipsRow: {
      display: "flex",
      alignItems: "center",
      gap: 6,
      marginTop: 8,
      flexWrap: "wrap" as const,
    },
    chipsLabel: {
      fontSize: 11,
      color: C.muted3,
    },
    chip: {
      padding: "3px 8px",
      background: C.panel2,
      border: `1px solid ${C.border}`,
      borderRadius: 6,
      color: C.purpleLt,
      fontSize: 11,
      cursor: "pointer",
    },

    // OUTPUTS PANEL
    outputsPanel: {
      flex: "0 0 38%",
      display: "flex",
      flexDirection: "column" as const,
      background: C.panel,
      borderTop: `1px solid ${C.border}`,
      overflow: "hidden",
    },
    outputsHeader: {
      display: "flex",
      alignItems: "center",
      gap: 8,
      padding: "8px 14px",
      borderBottom: `1px solid ${C.borderSoft}`,
      flexShrink: 0,
    },
    outputsTitle: {
      fontSize: 12,
      textTransform: "uppercase" as const,
      letterSpacing: "0.08em",
      color: C.muted3,
    },
    outputsStats: {
      fontSize: 11,
      color: C.muted3,
      marginLeft: 4,
    },
    outputsSpacer: { flex: 1 },
    pauseBtn: (paused: boolean) => ({
      padding: "3px 10px",
      background: "transparent",
      border: `1px solid ${paused ? C.coral : C.border}`,
      borderRadius: 5,
      color: paused ? C.coral : C.muted,
      fontSize: 11,
      cursor: "pointer",
    }),
    clearBtn: {
      padding: "3px 10px",
      background: "transparent",
      border: "none",
      color: C.muted3,
      fontSize: 11,
      cursor: "pointer",
    },
    queueList: {
      flex: 1,
      overflowY: "auto" as const,
      padding: "4px 0",
    },
    emptyState: {
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      height: "100%",
      padding: 24,
    },
    emptyBox: {
      border: `1px dashed ${C.border}`,
      borderRadius: 10,
      padding: "20px 28px",
      color: C.muted3,
      fontSize: 13,
      textAlign: "center" as const,
    },

    // RIGHT PANEL
    rightPanel: {
      width: 300,
      minWidth: 300,
      display: "flex",
      flexDirection: "column" as const,
      background: C.panel,
      borderLeft: `1px solid ${C.border}`,
      padding: "16px",
      overflowY: "auto" as const,
      gap: 18,
    },
    voiceHeaderRow: {
      display: "flex",
      alignItems: "center",
      gap: 10,
    },
    voiceHeaderAvatar: (idx: number) => ({
      width: 40,
      height: 40,
      borderRadius: "50%",
      background: avatarBg(idx < 0 ? 0 : idx),
      color: avatarFg(idx < 0 ? 0 : idx),
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      fontSize: 16,
      fontWeight: 600,
      flexShrink: 0,
    }),
    panelSectionHdr: {
      fontSize: 11,
      textTransform: "uppercase" as const,
      letterSpacing: "0.07em",
      color: C.muted3,
      marginBottom: 8,
    },
    moodGrid: {
      display: "grid",
      gridTemplateColumns: "1fr 1fr",
      gap: 6,
    },
    moodBtn: (selected: boolean) => ({
      height: 36,
      borderRadius: 8,
      border: `1px solid ${selected ? C.purple : C.border}`,
      background: selected ? "rgba(146,136,224,.15)" : C.panel2,
      color: selected ? C.purpleLt : C.muted,
      fontSize: 12,
      cursor: "pointer",
      transition: "all .12s",
    }),
    sliderRow: {
      marginBottom: 10,
    },
    sliderTop: {
      display: "flex",
      justifyContent: "space-between",
      marginBottom: 4,
    },
    sliderLabel: {
      fontSize: 12,
      color: C.muted,
    },
    sliderVal: {
      fontSize: 12,
      color: C.muted,
    },
    rangeInput: {
      width: "100%",
      accentColor: C.coral,
      cursor: "pointer",
    },
    formatSelect: {
      width: "100%",
      padding: "7px 10px",
      background: C.panel2,
      border: `1px solid ${C.border}`,
      borderRadius: 6,
      color: "#d4d4d4",
      fontSize: 13,
      outline: "none",
      cursor: "pointer",
    },
    ctaSpacer: { flex: 1 },
    ctaRow: {
      display: "flex",
      gap: 8,
      paddingTop: 4,
    },
    generateBtn: {
      flex: 1,
      height: 44,
      background: C.purple,
      border: "none",
      borderRadius: 10,
      color: "#fff",
      fontSize: 14.5,
      fontWeight: 600,
      cursor: "pointer",
      transition: "background .15s",
    },
    generateBtnDisabled: {
      flex: 1,
      height: 44,
      background: C.panel3,
      border: "none",
      borderRadius: 10,
      color: C.muted3,
      fontSize: 14.5,
      fontWeight: 600,
      cursor: "not-allowed",
    },
    addBtn: {
      width: 48,
      height: 44,
      background: C.panel2,
      border: `1px solid ${C.border}`,
      borderRadius: 10,
      color: C.purpleLt,
      fontSize: 18,
      cursor: "pointer",
      flexShrink: 0,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
    },

    // BOTTOM PLAYER
    bottomPlayer: {
      height: 54,
      minHeight: 54,
      display: "flex",
      alignItems: "center",
      gap: 12,
      padding: "0 16px",
      background: C.panel,
      borderTop: `1px solid ${C.border}`,
      flexShrink: 0,
    },
  };

  const LABELS = ["cười", "thở dài", "hắng giọng", "ngập ngừng", "nhấn mạnh"];
  const MOODS: Array<[string, string]> = [
    ["natural", "Tự nhiên"],
    ["storytelling", "Kể chuyện"],
    ["news", "Tin tức"],
    ["emotional", "Cảm xúc"],
  ];

  return (
    <div style={$.root} className="bss">
      <audio ref={audioRef} style={{ display: "none" }} />

      {/* ── TOP BAR ─────────────────────────────────────────────────── */}
      <div style={$.topBar}>
        <div style={$.speakerTile}>🔊</div>
        <div>
          <div style={$.topBarTitle}>Đọc văn bản</div>
          <div style={$.topBarSub}>Tổng hợp giọng nói tiếng Việt · 48 kHz · trên máy</div>
        </div>
        <div style={$.topBarSpacer} />
        <StatusPill status={status} info={info} voices={voices} />
        <button style={$.versionBadge}>v3-Turbo</button>
      </div>

      {/* ── MAIN ROW ────────────────────────────────────────────────── */}
      <div style={$.mainRow}>

        {/* ── LEFT PANEL ──────────────────────────────────────────── */}
        <div style={$.leftPanel}>
          <input
            type="text"
            placeholder="Tìm giọng đọc…"
            value={voiceSearch}
            onChange={(e) => setVoiceSearch(e.target.value)}
            style={$.searchBox}
          />
          <div style={$.sectionHeader}>Giọng đọc</div>
          <div style={$.voiceList}>
            {filteredVoices.map((v, i) => {
              const realIdx = voices.indexOf(v);
              const selected = v.id === voice;
              return (
                <div
                  key={v.id}
                  style={$.voiceRow(selected)}
                  onClick={() => setVoice(v.id)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => e.key === "Enter" && setVoice(v.id)}
                >
                  <div style={$.voiceAvatar(realIdx < 0 ? i : realIdx)}>
                    {v.label.charAt(0).toUpperCase()}
                  </div>
                  <div style={$.voiceName}>
                    <div style={$.voiceNameText}>{v.label}</div>
                    <div style={$.voiceDesc}>Giọng AI · Tiếng Việt</div>
                  </div>
                  {/* TODO: nghe thử giọng (chưa hỗ trợ) */}
                  <button
                    style={$.voicePreviewBtn}
                    title="TODO: nghe thử giọng (chưa hỗ trợ)"
                    disabled
                    onClick={(e) => e.stopPropagation()}
                  >
                    ▶
                  </button>
                </div>
              );
            })}
            {filteredVoices.length === 0 && (
              <div style={{ padding: "12px", fontSize: 12, color: C.muted3, textAlign: "center" }}>
                Không tìm thấy giọng nào
              </div>
            )}
          </div>
          {/* Clone section — TODO: nhân bản giọng (chưa hỗ trợ) */}
          <div style={$.cloneSection}>
            <div style={$.cloneSectionHdr}>Nhân bản giọng</div>
            <div style={$.cloneRow}>
              <button
                style={$.cloneBtn}
                title="TODO: nhân bản giọng (chưa hỗ trợ)"
                disabled
              >
                🎤 Ghi âm
              </button>
              <button
                style={$.cloneBtn}
                title="TODO: nhân bản giọng (chưa hỗ trợ)"
                disabled
              >
                📁 Tải lên
              </button>
            </div>
          </div>
        </div>

        {/* ── CENTER PANEL ────────────────────────────────────────── */}
        <div style={$.centerPanel}>
          {/* Text editor: top ~62% */}
          <div style={$.editorArea}>
            <div style={$.editorHeader}>
              <span style={$.editorLabel}>Văn bản</span>
              <span style={$.editorMeta}>
                {text.length} ký tự · ~{estDur(text.length)}
              </span>
            </div>
            <textarea
              ref={textareaRef}
              style={$.textarea}
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="Nhập văn bản tiếng Việt…"
              spellCheck={false}
              onFocus={(e) => { e.currentTarget.style.borderColor = C.teal; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = C.borderInset; }}
            />
            <div style={$.chipsRow}>
              <span style={$.chipsLabel}>Chèn nhãn:</span>
              {LABELS.map((lbl) => (
                <button key={lbl} style={$.chip} onClick={() => insertLabel(lbl)}>
                  [{lbl}]
                </button>
              ))}
            </div>
          </div>

          {/* Outputs panel: bottom ~38% */}
          <div style={$.outputsPanel}>
            <div style={$.outputsHeader}>
              <span style={$.outputsTitle}>Bản ghi</span>
              <span style={$.outputsStats}>
                {queue.stats.running} đang chạy · {queue.stats.queued} chờ · {queue.stats.done} xong
              </span>
              <div style={$.outputsSpacer} />
              <button
                style={$.pauseBtn(queue.paused)}
                onClick={() => queue.setPaused(!queue.paused)}
              >
                {queue.paused ? "Tiếp tục" : "Tạm dừng"}
              </button>
              <button
                style={$.clearBtn}
                onClick={queue.clearFinished}
                onMouseEnter={(e) => { e.currentTarget.style.color = C.coral; }}
                onMouseLeave={(e) => { e.currentTarget.style.color = C.muted3; }}
              >
                Xóa mục đã xong
              </button>
            </div>
            <div style={$.queueList}>
              {queue.items.length === 0 ? (
                <div style={$.emptyState}>
                  <div style={$.emptyBox}>
                    Chưa có bản ghi. Nhấn Tạo giọng nói để bắt đầu.
                  </div>
                </div>
              ) : (
                queue.items.map((item) => (
                  <QueueRow
                    key={item.id}
                    item={item}
                    selected={selectedItemId === item.id}
                    onSelect={() => setSelectedItemId(item.id)}
                    onCancel={() => queue.cancel(item.id)}
                    onSaveAs={() => saveAs(item)}
                    C={C}
                    MONO={MONO}
                  />
                ))
              )}
            </div>
          </div>
        </div>

        {/* ── RIGHT PANEL ─────────────────────────────────────────── */}
        <div style={$.rightPanel}>
          {/* Voice header */}
          <div style={$.voiceHeaderRow}>
            <div style={$.voiceHeaderAvatar(selectedVoiceIdx < 0 ? 0 : selectedVoiceIdx)}>
              {(selectedVoice?.label ?? "?").charAt(0).toUpperCase()}
            </div>
            <div>
              <div style={{ fontSize: 14, fontWeight: 600, color: "#d4d4d4" }}>
                {selectedVoice?.label ?? "Chưa chọn giọng"}
              </div>
              <div style={{ fontSize: 11, color: C.muted3, marginTop: 2 }}>Giọng AI · Tiếng Việt</div>
            </div>
          </div>

          {/* Mood */}
          <div>
            <div style={$.panelSectionHdr}>Sắc thái</div>
            <div style={$.moodGrid}>
              {MOODS.map(([val, lbl]) => (
                <button
                  key={val}
                  style={$.moodBtn(emotion === val)}
                  onClick={() => setEmotion(val)}
                >
                  {lbl}
                </button>
              ))}
            </div>
          </div>

          {/* Model params */}
          <div>
            <div style={$.panelSectionHdr}>Tham số mô hình</div>
            <SliderRow label="Temperature" value={temperature} min={0.1} max={1.5} step={0.05}
              onChange={setTemperature} C={C} />
            <SliderRow label="Top-K" value={topK} min={1} max={100} step={1}
              onChange={(v) => setTopK(Math.round(v))} C={C} />
            <SliderRow label="Top-P" value={topP} min={0.1} max={1} step={0.01}
              onChange={setTopP} C={C} />
            <SliderRow label="Rep. penalty" value={repPen} min={1} max={2} step={0.05}
              onChange={setRepPen} C={C} />
          </div>

          {/* Format */}
          <div>
            <div style={$.panelSectionHdr}>Định dạng</div>
            <select
              style={$.formatSelect}
              value={format}
              onChange={(e) => setFormat(e.target.value as "wav" | "mp3")}
            >
              <option value="mp3">MP3</option>
              <option value="wav">WAV (48 kHz)</option>
            </select>
          </div>

          {/* Spacer pushes CTA to bottom */}
          <div style={$.ctaSpacer} />

          {/* CTA */}
          <div style={$.ctaRow}>
            <button
              style={status === "ready" ? $.generateBtn : $.generateBtnDisabled}
              disabled={status !== "ready"}
              onClick={generate}
              onMouseEnter={(e) => { if (status === "ready") e.currentTarget.style.background = C.purpleLt; }}
              onMouseLeave={(e) => { if (status === "ready") e.currentTarget.style.background = C.purple; }}
            >
              🎙 Tạo giọng nói
            </button>
            <button
              style={$.addBtn}
              disabled={status !== "ready"}
              onClick={generate}
              title="Thêm vào hàng đợi"
            >
              +
            </button>
          </div>
        </div>
      </div>

      {/* ── BOTTOM PLAYER ───────────────────────────────────────────── */}
      <div style={$.bottomPlayer}>
        {hasSelection ? (
          <>
            <button
              onClick={togglePlayPause}
              style={{
                width: 36,
                height: 36,
                borderRadius: "50%",
                background: "rgba(234,124,105,.15)",
                border: `1px solid ${C.coral}`,
                color: C.coral,
                fontSize: 14,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                cursor: "pointer",
                flexShrink: 0,
              }}
            >
              {playing ? "⏸" : "▶"}
            </button>
            <div style={{ minWidth: 0, flexShrink: 0 }}>
              <div style={{ fontSize: 12, color: "#d4d4d4", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 160 }}>
                {(selectedItem?.label ?? "").slice(0, 40)}{(selectedItem?.label ?? "").length > 40 ? "…" : ""}
              </div>
              <div style={{ fontSize: 10, color: C.muted3, fontFamily: MONO }}>
                {selectedItem?.voiceLabel ?? ""}
              </div>
            </div>
            <div style={{ fontFamily: MONO, fontSize: 11, color: C.muted, flexShrink: 0 }}>
              {fmtTime(currentTime)} / {duration ? fmtTime(duration) : "0:00"}
            </div>
            <input
              type="range"
              style={{ flex: 1, accentColor: C.coral, cursor: "pointer", height: 4 }}
              min={0}
              max={100}
              step={0.1}
              value={duration ? (currentTime / duration) * 100 : 0}
              onChange={(e) => seekTo(parseFloat(e.target.value))}
            />
            <span style={{ fontSize: 16, color: C.muted, flexShrink: 0 }}>🔈</span>
            <input
              type="range"
              style={{ width: 80, accentColor: C.coral, cursor: "pointer" }}
              min={0}
              max={1}
              step={0.01}
              value={volume}
              onChange={(e) => setVolumeVal(parseFloat(e.target.value))}
            />
          </>
        ) : (
          <div style={{ flex: 1, textAlign: "center", color: C.muted3, fontSize: 13 }}>
            Chưa chọn bản ghi
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Status Pill ──────────────────────────────────────────────────────────────
function StatusPill({
  status,
  info,
  voices,
}: {
  status: "starting" | "ready" | "error";
  info: ServerInfo | null;
  voices: Voice[];
}) {
  if (status === "ready") {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          border: `1px solid ${C.teal}`,
          background: "rgba(80,209,170,.1)",
          borderRadius: 20,
          padding: "4px 10px",
          fontSize: 12,
          color: C.teal,
          flexShrink: 0,
        }}
      >
        <span
          style={{
            width: 7,
            height: 7,
            borderRadius: "50%",
            background: C.teal,
            flexShrink: 0,
          }}
        />
        Sẵn sàng · {info?.pool_size ?? 0} luồng · {voices.length} giọng
      </div>
    );
  }
  if (status === "starting") {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          border: "1px solid #FFB572",
          background: "rgba(255,181,114,.1)",
          borderRadius: 20,
          padding: "4px 10px",
          fontSize: 12,
          color: "#FFB572",
          flexShrink: 0,
        }}
      >
        <span
          style={{
            width: 7,
            height: 7,
            borderRadius: "50%",
            background: "#FFB572",
            flexShrink: 0,
          }}
        />
        Đang khởi động…
      </div>
    );
  }
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        border: `1px solid ${C.coral}`,
        background: "rgba(234,124,105,.1)",
        borderRadius: 20,
        padding: "4px 10px",
        fontSize: 12,
        color: C.coral,
        flexShrink: 0,
      }}
    >
      <span
        style={{
          width: 7,
          height: 7,
          borderRadius: "50%",
          background: C.coral,
          flexShrink: 0,
        }}
      />
      Lỗi kết nối
    </div>
  );
}

// ─── Slider Row ───────────────────────────────────────────────────────────────
function SliderRow({
  label,
  value,
  min,
  max,
  step,
  onChange,
  C: colors,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  C: typeof C;
}) {
  const display = Number.isInteger(value) ? value : value.toFixed(step < 0.05 ? 2 : 2);
  return (
    <div style={{ marginBottom: 10 }}>
      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 4 }}>
        <span style={{ fontSize: 12, color: colors.muted }}>{label}</span>
        <span style={{ fontSize: 12, color: colors.muted }}>{display}</span>
      </div>
      <input
        type="range"
        style={{ width: "100%", accentColor: colors.coral, cursor: "pointer" }}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
      />
    </div>
  );
}

// ─── Queue Row ────────────────────────────────────────────────────────────────
function QueueRow({
  item,
  selected,
  onSelect,
  onCancel,
  onSaveAs,
  C: colors,
  MONO: mono,
}: {
  item: QueueItem;
  selected: boolean;
  onSelect: () => void;
  onCancel: () => void;
  onSaveAs: () => void;
  C: typeof C;
  MONO: string;
}) {
  const isRunning = item.status === "running";
  const isDone = item.status === "done";
  const isFailed = item.status === "failed";
  const isCancelled = item.status === "cancelled";
  const isQueued = item.status === "queued";

  return (
    <div
      style={{
        position: "relative",
        padding: "6px 14px",
        display: "flex",
        alignItems: "center",
        gap: 10,
        minHeight: 48,
        background: selected ? "rgba(234,124,105,.07)" : "transparent",
        borderLeft: selected ? `2px solid ${colors.coral}` : "2px solid transparent",
        transition: "background .12s",
      }}
    >
      {/* Status circle */}
      <div
        style={{
          width: 28,
          height: 28,
          borderRadius: "50%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
          fontSize: 13,
          cursor: isDone ? "pointer" : "default",
          background: isDone
            ? "rgba(234,124,105,.15)"
            : isRunning
            ? "rgba(80,209,170,.12)"
            : isFailed
            ? "rgba(234,124,105,.12)"
            : "rgba(139,143,158,.1)",
          border: isDone
            ? `1px solid ${colors.coral}`
            : isRunning
            ? `1px solid ${colors.teal}`
            : isFailed
            ? `1px solid ${colors.coral}`
            : `1px solid ${colors.border}`,
          animation: isRunning ? "bss-spin 1.2s linear infinite" : undefined,
        }}
        onClick={isDone ? onSelect : undefined}
        title={isDone ? "Phát" : undefined}
      >
        {isDone ? (
          <span style={{ color: colors.coral }}>▶</span>
        ) : isRunning ? (
          <span style={{ color: colors.teal, fontSize: 10 }}>⟳</span>
        ) : isFailed ? (
          <span style={{ color: colors.coral }}>✕</span>
        ) : isCancelled ? (
          <span style={{ color: colors.muted3 }}>✕</span>
        ) : (
          <span style={{ color: colors.muted3, fontSize: 10 }}>⏱</span>
        )}
      </div>

      {/* Text snippet */}
      <div
        style={{
          flex: 1,
          fontSize: 13,
          color: "#ccc",
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
          minWidth: 0,
        }}
      >
        {item.label.slice(0, 60)}
      </div>

      {/* Status pill */}
      <StatusChip status={item.status} colors={colors} />

      {/* Duration */}
      {isDone && (
        <span
          style={{
            fontSize: 10,
            color: colors.muted3,
            fontFamily: mono,
            flexShrink: 0,
          }}
        >
          {item.durationS != null ? fmtTime(item.durationS) : ""}
        </span>
      )}

      {/* Download */}
      {isDone && (
        <button
          onClick={onSaveAs}
          title="Tải xuống"
          style={{
            background: "transparent",
            border: "none",
            color: colors.muted3,
            cursor: "pointer",
            fontSize: 14,
            padding: "0 2px",
            flexShrink: 0,
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = colors.teal; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = colors.muted3; }}
        >
          ↓
        </button>
      )}

      {/* Cancel / Remove */}
      {(isQueued || isRunning || isDone || isFailed || isCancelled) && (
        <button
          onClick={onCancel}
          title="Xóa"
          style={{
            background: "transparent",
            border: "none",
            color: colors.muted3,
            cursor: "pointer",
            fontSize: 13,
            padding: "0 2px",
            flexShrink: 0,
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = colors.coral; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = colors.muted3; }}
        >
          ✕
        </button>
      )}

      {/* Progress bar while running */}
      {isRunning && (
        <div
          style={{
            position: "absolute",
            bottom: 0,
            left: 0,
            right: 0,
            height: 3,
            background: `linear-gradient(90deg, ${colors.teal} 0%, transparent 100%)`,
            animation: "bss-progress 1.8s ease-in-out infinite",
            borderRadius: "0 0 2px 2px",
          }}
        />
      )}
    </div>
  );
}

function StatusChip({
  status,
  colors,
}: {
  status: string;
  colors: typeof C;
}) {
  let text = "";
  let bg = "transparent";
  let color: string = colors.muted3;
  if (status === "running") { text = "Đang tạo…"; color = "#FFB572"; bg = "rgba(255,181,114,.1)"; }
  else if (status === "done") { text = "Hoàn tất"; color = colors.teal as string; bg = "rgba(80,209,170,.1)"; }
  else if (status === "failed") { text = "Lỗi"; color = colors.coral as string; bg = "rgba(234,124,105,.1)"; }
  else if (status === "queued") { text = "Trong hàng"; color = colors.muted3 as string; }
  else if (status === "cancelled") { text = "Đã hủy"; color = colors.muted3 as string; }

  return (
    <div
      style={{
        fontSize: 10,
        borderRadius: 4,
        padding: "2px 6px",
        background: bg,
        color,
        flexShrink: 0,
        whiteSpace: "nowrap",
      }}
    >
      {text}
    </div>
  );
}
