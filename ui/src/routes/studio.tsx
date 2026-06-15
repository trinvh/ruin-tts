import { useCallback, useEffect, useRef, useState } from "react";
import { getInfo, getVoices, type ServerInfo, type Voice } from "../api";
import { buildSubmit, useQueue, type QueueItem } from "../queue";
import { copyFile, isTauri, revealInDir, saveAsDialog } from "../platform";
import { useTtsSettings } from "../ttsSettings";
import { Dropdown } from "../components/Dropdown";
import { Help } from "../components/Help";
import { ClonePanel } from "../components/ClonePanel";
import { QueuePanel } from "../components/QueuePanel";

type Status = "starting" | "ready" | "down";
const SAMPLE =
  "[cười] Trời ơi, cái giọng nó tự nhiên mà mượt mà dã man, nghe không khác gì người thật luôn.";

export function StudioPage() {
  const { outputDir, concurrency } = useTtsSettings();
  const [status, setStatus] = useState<Status>("starting");
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
  const voicesLoaded = useRef(false);

  const [text, setText] = useState(SAMPLE);
  const [voice, setVoice] = useState("");
  const [clone, setClone] = useState<{ refId: string; name: string } | null>(null);
  const [emotion, setEmotion] = useState("natural");
  const [temperature, setTemperature] = useState(0.8);
  const [topK, setTopK] = useState(25);
  const [topP, setTopP] = useState(0.95);
  const [repPen, setRepPen] = useState(1.2);
  const [format, setFormat] = useState<"wav" | "mp3">("mp3");

  const textRef = useRef<HTMLTextAreaElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);

  const queue = useQueue(outputDir, concurrency);

  // Poll server health; load voices exactly once (fixes the voice-reset bug).
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
        if (alive) setStatus((s) => (s === "ready" ? "down" : "starting"));
      }
    };
    tick();
    const h = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(h);
    };
  }, []);

  const insertCue = useCallback((token: string) => {
    const el = textRef.current;
    if (!el) {
      setText((t) => `${t} ${token}`);
      return;
    }
    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? el.value.length;
    setText((prev) => prev.slice(0, start) + token + prev.slice(end));
    requestAnimationFrame(() => {
      el.focus();
      const pos = start + token.length;
      el.setSelectionRange(pos, pos);
    });
  }, []);

  const cloning = clone !== null;
  const voiceOptions = cloning
    ? [{ value: "__clone__", label: `🎙 ${clone!.name}` }]
    : voices.map((v) => ({ value: v.id, label: v.label }));
  const voiceValue = cloning ? "__clone__" : voice;
  const voiceLabel = cloning
    ? `clone · ${clone!.name}`
    : voices.find((v) => v.id === voice)?.label ?? voice;

  const generate = useCallback(() => {
    if (!text.trim()) return;
    const params = {
      text,
      voice: cloning ? undefined : voice || undefined,
      ref_id: cloning ? clone!.refId : undefined,
      emotion,
      temperature,
      top_k: topK,
      top_p: topP,
      repetition_penalty: repPen,
      format,
    };
    queue.enqueue(buildSubmit(params, voiceLabel));
  }, [text, cloning, clone, voice, emotion, temperature, topK, topP, repPen, format, voiceLabel, queue]);

  const play = useCallback((it: QueueItem) => {
    if (audioRef.current && it.url) {
      audioRef.current.src = it.url;
      audioRef.current.play().catch(() => {});
    }
  }, []);

  const saveAs = useCallback(async (it: QueueItem) => {
    const fname = `vieneu.${it.format}`;
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

  const reveal = useCallback((it: QueueItem) => {
    if (it.savedPath) void revealInDir(it.savedPath);
  }, []);

  return (
    <div className="mx-auto w-full max-w-[1120px]">
      <div className="mb-4 flex items-end justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold text-ink">Đọc văn bản</h2>
          <p className="mt-1 text-sm text-muted">
            Tổng hợp giọng nói tiếng Việt v3-Turbo, 48 kHz, ngay trên máy.
          </p>
        </div>
        <StatusPill status={status} info={info} />
      </div>

      <div className="grid">
        <div className="col">
          <section className="panel compose">
            <label className="field-label">Văn bản</label>
            <textarea
              ref={textRef}
              className="text"
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="Nhập văn bản tiếng Việt…"
              spellCheck={false}
            />
            <div className="cue-row">
              {["[cười]", "[thở dài]", "[hắng giọng]"].map((c) => (
                <button key={c} className="cue" onClick={() => insertCue(c)}>
                  {c}
                </button>
              ))}
              <span className="count">{text.length} ký tự</span>
            </div>

            <div className="controls">
              <div className="control">
                <label className="field-label">Giọng đọc</label>
                <Dropdown
                  value={voiceValue}
                  options={voiceOptions}
                  onChange={setVoice}
                  disabled={cloning}
                  placeholder="Chọn giọng…"
                />
              </div>
              <div className="control">
                <label className="field-label">
                  Sắc thái
                  <Help title="Sắc thái">
                    Chỉ áp dụng cho <b>giọng nhân bản</b>. Giọng có sẵn đã được cố định sắc thái nên
                    lựa chọn này không thay đổi kết quả.
                  </Help>
                </label>
                <div className={`segmented ${cloning ? "" : "seg-off"}`}>
                  {[
                    ["natural", "Tự nhiên"],
                    ["storytelling", "Kể chuyện"],
                  ].map(([v, l]) => (
                    <button
                      key={v}
                      className={emotion === v ? "seg on" : "seg"}
                      disabled={!cloning}
                      onClick={() => setEmotion(v)}
                    >
                      {l}
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="sliders">
              <Slider
                label="Temperature"
                value={temperature}
                min={0}
                max={1.5}
                step={0.05}
                onChange={setTemperature}
                help={[
                  "Temperature",
                  "Độ ngẫu nhiên. Thấp (0) = ổn định, lặp lại; cao = đa dạng, biểu cảm hơn nhưng dễ lỗi. Mặc định 0.8.",
                ]}
              />
              <Slider
                label="Top-K"
                value={topK}
                min={0}
                max={100}
                step={1}
                onChange={(v) => setTopK(Math.round(v))}
                help={["Top-K", "Chỉ lấy K token xác suất cao nhất ở mỗi bước. Nhỏ = an toàn hơn. 0 = tắt. Mặc định 25."]}
              />
              <Slider
                label="Top-P"
                value={topP}
                min={0.1}
                max={1}
                step={0.01}
                onChange={setTopP}
                help={["Top-P (nucleus)", "Lấy nhóm token nhỏ nhất có tổng xác suất ≥ P. 1.0 = tắt. Mặc định 0.95."]}
              />
              <Slider
                label="Rep. penalty"
                value={repPen}
                min={1}
                max={2}
                step={0.05}
                onChange={setRepPen}
                help={["Repetition penalty", "Phạt token đã xuất hiện để giảm lặp âm. 1.0 = tắt. Mặc định 1.2."]}
              />
            </div>

            <div className="compose-foot">
              <div className="fmt">
                <label className="field-label">Định dạng</label>
                <Dropdown
                  value={format}
                  options={[
                    { value: "mp3", label: "MP3 (192 kbps)" },
                    { value: "wav", label: "WAV (48 kHz)" },
                  ]}
                  onChange={(v) => setFormat(v as "wav" | "mp3")}
                />
              </div>
              <button className="generate" disabled={status !== "ready"} onClick={generate}>
                ＋ Thêm vào hàng đợi
              </button>
            </div>
          </section>

          <ClonePanel
            active={clone}
            onCloned={(refId, name) => setClone({ refId, name })}
            onClear={() => setClone(null)}
          />
        </div>

        <div className="col">
          <QueuePanel
            items={queue.items}
            paused={queue.paused}
            setPaused={queue.setPaused}
            cancel={queue.cancel}
            cancelAll={queue.cancelAll}
            clearFinished={queue.clearFinished}
            stats={queue.stats}
            onPlay={play}
            onSaveAs={saveAs}
            onReveal={reveal}
            canReveal={isTauri()}
          />
          <audio ref={audioRef} controls className="player queue-player" />
        </div>
      </div>
    </div>
  );
}

function StatusPill({ status, info }: { status: Status; info: ServerInfo | null }) {
  const text =
    status === "ready"
      ? `Sẵn sàng · ${info?.pool_size ?? "?"} luồng · ${info?.voices ?? "?"} giọng`
      : status === "starting"
      ? "Đang khởi động máy chủ…"
      : "Mất kết nối máy chủ";
  return (
    <div className={`pill ${status}`}>
      <span className="dot" />
      {text}
    </div>
  );
}

function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  help: [string, React.ReactNode];
}) {
  return (
    <label className="slider">
      <span className="slider-top">
        <span>
          {props.label}
          <Help title={props.help[0]}>{props.help[1]}</Help>
        </span>
        <b>{props.value}</b>
      </span>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(e) => props.onChange(parseFloat(e.target.value))}
      />
    </label>
  );
}
