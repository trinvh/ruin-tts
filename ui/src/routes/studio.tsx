import { useCallback, useEffect, useRef, useState, type ChangeEvent } from "react";
import { getInfo, getVoices, synthDirect, type ServerInfo, type SynthParams, type Voice } from "../api";
import { buildSubmit, useQueue, type QueueItem } from "../queue";
import { copyFile, isTauri, saveAsDialog } from "../platform";
import { useTtsSettings } from "../ttsSettings";
import {
  addClone,
  ensureRefId,
  loadClones,
  removeClone,
  type ClonedVoice,
} from "../clonedVoices";
import { toWav } from "../wav";
import { C, FONT, MONO, injectStudioStyles } from "../studio-shell/theme";

type Status = "starting" | "ready" | "error";

const PREVIEW_TEXT = "Xin chào, đây là giọng đọc thử của Beesoft Studio.";

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

// Page-local keyframes (recording-dot pulse + new-clone flash). Injected once,
// kept here so the shared theme stylesheet stays untouched.
const TTS_STYLE_ID = "beesoft-tts-styles";
function injectTtsStyles(): void {
  if (typeof document === "undefined") return;
  if (document.getElementById(TTS_STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = TTS_STYLE_ID;
  style.textContent = `
@keyframes bss-rec-pulse{0%{box-shadow:0 0 0 0 rgba(234,124,105,.55);}70%{box-shadow:0 0 0 7px rgba(234,124,105,0);}100%{box-shadow:0 0 0 0 rgba(234,124,105,0);}}
.bss .bss-rec-dot{animation:bss-rec-pulse 1.1s ease-out infinite;}
@keyframes bss-flash-kf{0%{background:rgba(80,209,170,.32);}100%{background:rgba(80,209,170,0);}}
.bss .bss-flash{animation:bss-flash-kf 2.4s ease-out 1;border-radius:8px;}
`;
  document.head.appendChild(style);
}

// ─── Main page ────────────────────────────────────────────────────────────────
export function StudioPage() {
  injectStudioStyles();
  injectTtsStyles();

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

  // Voice cloning + preview state
  const [clones, setClones] = useState<ClonedVoice[]>(() => loadClones());
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);
  const [cloning, setCloning] = useState(false);
  const [recSeconds, setRecSeconds] = useState(0);
  const [justSaved, setJustSaved] = useState<{ id: string; name: string } | null>(null);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const mediaRecRef = useRef<MediaRecorder | null>(null);
  const recChunksRef = useRef<Blob[]>([]);
  const recTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const savedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  // Clean up recording / confirmation timers on unmount.
  useEffect(() => {
    return () => {
      if (recTimerRef.current) clearInterval(recTimerRef.current);
      if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
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

  const selectedClone = clones.find((c) => c.id === voice) ?? null;
  const selectedVoice = voices.find((v) => v.id === voice);
  const voiceLabel = selectedClone?.name ?? selectedVoice?.label ?? voice;
  const selectedItem = queue.items.find((x) => x.id === selectedItemId) ?? null;
  const selectedVoiceIdx = voices.findIndex((v) => v.id === voice);

  const q = voiceSearch.trim().toLowerCase();
  const filteredVoices = q
    ? voices.filter((v) => v.label.toLowerCase().includes(q))
    : voices;
  const filteredClones = q
    ? clones.filter((c) => c.name.toLowerCase().includes(q))
    : clones;

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

  const generate = useCallback(async () => {
    if (!text.trim()) return;
    try {
      let params: SynthParams;
      if (selectedClone) {
        const ref_id = await ensureRefId(selectedClone);
        params = {
          text,
          ref_id,
          emotion,
          temperature,
          top_k: topK,
          top_p: topP,
          repetition_penalty: repPen,
          format,
        };
      } else {
        params = {
          text,
          voice: voice || undefined,
          emotion,
          temperature,
          top_k: topK,
          top_p: topP,
          repetition_penalty: repPen,
          format,
        };
      }
      queue.enqueue(buildSubmit(params, voiceLabel));
    } catch (e) {
      alert("Không tạo được ref giọng nhân bản: " + (e instanceof Error ? e.message : String(e)));
    }
  }, [text, voice, selectedClone, emotion, temperature, topK, topP, repPen, format, voiceLabel, queue]);

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

  // ── Voice preview (nghe thử) ───────────────────────────────────────────────
  const preview = useCallback(
    async (target: { voiceId: string } | { clone: ClonedVoice }) => {
      if (status !== "ready" || previewing) return;
      const id = "voiceId" in target ? target.voiceId : target.clone.id;
      setPreviewing(id);
      try {
        let params: SynthParams;
        const common = {
          text: PREVIEW_TEXT,
          emotion,
          temperature,
          top_k: topK,
          top_p: topP,
          repetition_penalty: repPen,
          format: "wav" as const,
        };
        if ("clone" in target) {
          const ref_id = await ensureRefId(target.clone);
          params = { ...common, ref_id };
        } else {
          params = { ...common, voice: target.voiceId };
        }
        const blob = await synthDirect(params);
        const url = URL.createObjectURL(blob);
        const a = new Audio(url);
        a.onended = () => URL.revokeObjectURL(url);
        await a.play();
      } catch (e) {
        alert("Không nghe thử được giọng: " + (e instanceof Error ? e.message : String(e)));
      } finally {
        setPreviewing(null);
      }
    },
    [status, previewing, emotion, temperature, topK, topP, repPen],
  );

  // ── Add a clone (shared by record + upload) ────────────────────────────────
  const commitClone = useCallback(async (wav: Blob, defaultName: string) => {
    const name = prompt("Tên giọng nhân bản:", defaultName);
    if (!name) return;
    setCloning(true);
    try {
      const before = loadClones().map((c) => c.id);
      const next = await addClone(name, wav);
      setClones(next);
      // Identify the newly added clone, select it, and flash a confirmation.
      const added = next.find((c) => !before.includes(c.id)) ?? next[next.length - 1];
      if (added) {
        setVoice(added.id);
        setJustSaved({ id: added.id, name: added.name });
        if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
        savedTimerRef.current = setTimeout(() => setJustSaved(null), 4000);
      }
    } catch (e) {
      alert("Không nhân bản được giọng: " + (e instanceof Error ? e.message : String(e)));
    } finally {
      setCloning(false);
    }
  }, []);

  // ── Mic recording ──────────────────────────────────────────────────────────
  const toggleRecord = useCallback(async () => {
    if (recording) {
      mediaRecRef.current?.stop();
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mr = new MediaRecorder(stream);
      recChunksRef.current = [];
      mr.ondataavailable = (e) => {
        if (e.data.size > 0) recChunksRef.current.push(e.data);
      };
      mr.onstop = async () => {
        stream.getTracks().forEach((t) => t.stop());
        if (recTimerRef.current) {
          clearInterval(recTimerRef.current);
          recTimerRef.current = null;
        }
        setRecording(false);
        const raw = new Blob(recChunksRef.current, { type: mr.mimeType || "audio/webm" });
        try {
          const wav = await toWav(raw);
          await commitClone(wav, "Giọng của tôi");
        } catch (e) {
          alert("Không xử lý được bản ghi: " + (e instanceof Error ? e.message : String(e)));
        }
      };
      mediaRecRef.current = mr;
      mr.start();
      setRecording(true);
      setRecSeconds(0);
      if (recTimerRef.current) clearInterval(recTimerRef.current);
      recTimerRef.current = setInterval(() => setRecSeconds((s) => s + 1), 1000);
    } catch (e) {
      alert(
        "Không truy cập được micro — cấp quyền micro cho ứng dụng rồi thử lại. (" +
          (e instanceof Error ? e.message : String(e)) +
          ")",
      );
    }
  }, [recording, commitClone]);

  // ── File upload ────────────────────────────────────────────────────────────
  const onUploadFile = useCallback(
    async (e: ChangeEvent<HTMLInputElement>) => {
      const f = e.target.files?.[0];
      e.target.value = "";
      if (!f) return;
      try {
        const wav = await toWav(f);
        await commitClone(wav, f.name.replace(/\.[^.]+$/, ""));
      } catch (err) {
        alert("Không đọc được tệp âm thanh: " + (err instanceof Error ? err.message : String(err)));
      }
    },
    [commitClone],
  );

  const deleteClone = useCallback(
    (cv: ClonedVoice) => {
      const next = removeClone(cv.id);
      setClones(next);
      if (voice === cv.id) setVoice(voices[0]?.id ?? "");
    },
    [voice, voices],
  );

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
    voicePreviewBtn: (enabled: boolean, active: boolean) => ({
      width: 22,
      height: 22,
      borderRadius: "50%",
      background: active ? "rgba(80,209,170,.18)" : C.panel2,
      border: "none",
      color: active ? C.teal : "#ccc",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      fontSize: 9,
      opacity: enabled ? 1 : 0.3,
      cursor: enabled ? "pointer" : "not-allowed",
      flexShrink: 0,
      padding: 0,
      animation: active ? "bss-spin 1.2s linear infinite" : undefined,
    }),
    cloneDeleteBtn: {
      width: 22,
      height: 22,
      borderRadius: "50%",
      background: "transparent",
      border: "none",
      color: C.muted3,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      fontSize: 11,
      cursor: "pointer",
      flexShrink: 0,
      padding: 0,
    },
    cloneBadge: {
      fontSize: 10,
      color: C.purpleLt,
      marginTop: 1,
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
    cloneHelper: {
      fontSize: 11,
      lineHeight: 1.45,
      color: C.muted3,
      marginBottom: 8,
    },
    cloneHint: {
      fontSize: 10,
      color: C.muted3,
      marginTop: 6,
    },
    cloneRow: {
      display: "flex",
      gap: 6,
    },
    cloneBtn: (busy: boolean, rec: boolean) => ({
      flex: 1,
      padding: "5px 0",
      background: rec ? "rgba(255,124,163,.14)" : C.panel2,
      border: `1px solid ${rec ? C.pink : C.border}`,
      borderRadius: 6,
      color: rec ? C.pink : C.muted,
      fontSize: 11,
      cursor: busy ? "not-allowed" : "pointer",
      opacity: busy && !rec ? 0.5 : 1,
    }),
    recBar: {
      display: "flex",
      alignItems: "center",
      gap: 8,
      padding: "8px 10px",
      borderRadius: 8,
      border: `1px solid ${C.coral}`,
      background: "rgba(234,124,105,.12)",
    },
    recDot: {
      width: 9,
      height: 9,
      borderRadius: "50%",
      background: C.coral,
      flexShrink: 0,
      boxShadow: "0 0 0 0 rgba(234,124,105,.6)",
    },
    recLabel: {
      fontSize: 11.5,
      fontWeight: 600,
      color: C.coral,
      fontFamily: MONO,
    },
    recStopBtn: {
      padding: "4px 10px",
      borderRadius: 6,
      border: `1px solid ${C.coral}`,
      background: C.coral,
      color: "#fff",
      fontSize: 11,
      fontWeight: 600,
      cursor: "pointer",
      flexShrink: 0,
    },
    savedNote: {
      marginTop: 8,
      fontSize: 11,
      fontWeight: 600,
      color: C.teal,
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
      background: selected ? "rgba(146,136,224,.22)" : C.panel2,
      color: selected ? "#fff" : C.muted,
      fontSize: 12,
      fontWeight: selected ? 600 : 400,
      cursor: "pointer",
      transition: "all .12s",
    }),
    moodNote: {
      marginTop: 8,
      fontSize: 10.5,
      lineHeight: 1.45,
      color: C.muted3,
    },
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
  // [presetKey, label, emotion token, temperature preset]. The model only
  // distinguishes natural vs non-natural for the emotion token, so the audible
  // variety between non-natural moods comes from the temperature preset.
  const MOODS: Array<[string, string, string, number]> = [
    ["natural", "Tự nhiên", "natural", 0.8],
    ["storytelling", "Kể chuyện", "storytelling", 0.7],
    ["news", "Tin tức", "storytelling", 0.6],
    ["emotional", "Cảm xúc", "storytelling", 0.95],
  ];
  const activeMood = MOODS.find(
    ([, , em, temp]) => em === emotion && Math.abs(temp - temperature) < 1e-6,
  )?.[0];
  const applyMood = (em: string, temp: number) => {
    setEmotion(em);
    setTemperature(temp);
  };

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
              const isPreviewing = previewing === v.id;
              const previewEnabled = status === "ready" && (!previewing || isPreviewing);
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
                  <button
                    style={$.voicePreviewBtn(previewEnabled, isPreviewing)}
                    title={status === "ready" ? "Nghe thử giọng" : "Máy chủ chưa sẵn sàng"}
                    disabled={!previewEnabled}
                    onClick={(e) => {
                      e.stopPropagation();
                      void preview({ voiceId: v.id });
                    }}
                  >
                    {isPreviewing ? "⟳" : "▶"}
                  </button>
                </div>
              );
            })}

            {/* Cloned voices */}
            {filteredClones.map((cv) => {
              const selected = cv.id === voice;
              const isPreviewing = previewing === cv.id;
              const previewEnabled = status === "ready" && (!previewing || isPreviewing);
              return (
                <div
                  key={cv.id}
                  style={$.voiceRow(selected)}
                  className={justSaved?.id === cv.id ? "bss-flash" : undefined}
                  onClick={() => setVoice(cv.id)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => e.key === "Enter" && setVoice(cv.id)}
                >
                  <div style={$.voiceAvatar(0)}>{cv.name.charAt(0).toUpperCase()}</div>
                  <div style={$.voiceName}>
                    <div style={$.voiceNameText}>{cv.name}</div>
                    <div style={$.cloneBadge}>đã nhân bản</div>
                  </div>
                  <button
                    style={$.voicePreviewBtn(previewEnabled, isPreviewing)}
                    title={status === "ready" ? "Nghe thử giọng" : "Máy chủ chưa sẵn sàng"}
                    disabled={!previewEnabled}
                    onClick={(e) => {
                      e.stopPropagation();
                      void preview({ clone: cv });
                    }}
                  >
                    {isPreviewing ? "⟳" : "▶"}
                  </button>
                  <button
                    style={$.cloneDeleteBtn}
                    title="Xóa giọng nhân bản"
                    onClick={(e) => {
                      e.stopPropagation();
                      deleteClone(cv);
                    }}
                    onMouseEnter={(e) => { e.currentTarget.style.color = C.coral; }}
                    onMouseLeave={(e) => { e.currentTarget.style.color = C.muted3; }}
                  >
                    🗑
                  </button>
                </div>
              );
            })}

            {filteredVoices.length === 0 && filteredClones.length === 0 && (
              <div style={{ padding: "12px", fontSize: 12, color: C.muted3, textAlign: "center" }}>
                Không tìm thấy giọng nào
              </div>
            )}
          </div>
          {/* Clone section */}
          <div style={$.cloneSection}>
            <div style={$.cloneSectionHdr}>Nhân bản giọng</div>
            <div style={$.cloneHelper}>
              Ghi từ micro → lưu thành một giọng trong thư viện bên trái, dùng lại được sau.
            </div>

            {recording ? (
              <div style={$.recBar}>
                <span style={$.recDot} className="bss-rec-dot" />
                <span style={$.recLabel}>● Đang ghi… {fmtTime(recSeconds)}</span>
                <div style={{ flex: 1 }} />
                <button style={$.recStopBtn} onClick={() => void toggleRecord()}>
                  ■ Dừng &amp; lưu
                </button>
              </div>
            ) : (
              <>
                <div style={$.cloneRow}>
                  <button
                    style={$.cloneBtn(cloning, false)}
                    title="Ghi âm giọng mẫu"
                    disabled={cloning}
                    onClick={() => void toggleRecord()}
                  >
                    🎤 Ghi âm
                  </button>
                  <button
                    style={$.cloneBtn(cloning, false)}
                    title="Tải lên tệp âm thanh mẫu"
                    disabled={cloning}
                    onClick={() => fileInputRef.current?.click()}
                  >
                    📁 Tải lên
                  </button>
                </div>
                <div style={$.cloneHint}>Ghi 3–5 giây giọng mẫu</div>
              </>
            )}

            {cloning && (
              <div style={$.cloneHint}>Đang xử lý giọng mẫu…</div>
            )}
            {justSaved && (
              <div style={$.savedNote}>✓ Đã lưu giọng “{justSaved.name}” vào thư viện</div>
            )}

            <input
              ref={fileInputRef}
              type="file"
              accept="audio/*"
              style={{ display: "none" }}
              onChange={onUploadFile}
            />
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
                    onCancel={() => void queue.remove(item.id)}
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
            <div style={$.voiceHeaderAvatar(selectedClone ? 0 : selectedVoiceIdx < 0 ? 0 : selectedVoiceIdx)}>
              {(voiceLabel || "?").charAt(0).toUpperCase()}
            </div>
            <div>
              <div style={{ fontSize: 14, fontWeight: 600, color: "#d4d4d4" }}>
                {selectedClone ? selectedClone.name : selectedVoice?.label ?? "Chưa chọn giọng"}
              </div>
              <div style={{ fontSize: 11, color: selectedClone ? C.purpleLt : C.muted3, marginTop: 2 }}>
                {selectedClone ? "Giọng nhân bản · Tiếng Việt" : "Giọng AI · Tiếng Việt"}
              </div>
            </div>
          </div>

          {/* Mood */}
          <div>
            <div style={$.panelSectionHdr}>Sắc thái</div>
            <div style={$.moodGrid}>
              {MOODS.map(([key, lbl, em, temp]) => (
                <button
                  key={key}
                  style={$.moodBtn(activeMood === key)}
                  onClick={() => applyMood(em, temp)}
                >
                  {lbl}
                </button>
              ))}
            </div>
            <div style={$.moodNote}>
              Với giọng có sẵn, cảm xúc rõ nhất khi chèn nhãn [cười]/[thở dài]… vào văn bản;
              sắc thái chỉnh tông qua tham số.
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
              onClick={() => void generate()}
              onMouseEnter={(e) => { if (status === "ready") e.currentTarget.style.background = C.purpleLt; }}
              onMouseLeave={(e) => { if (status === "ready") e.currentTarget.style.background = C.purple; }}
            >
              🎙 Tạo giọng nói
            </button>
            <button
              style={$.addBtn}
              disabled={status !== "ready"}
              onClick={() => void generate()}
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
