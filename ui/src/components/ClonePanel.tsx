import { useRef, useState } from "react";
import { cloneVoice } from "../api";
import { WavRecorder } from "../recorder";

const SAMPLE_TEXTS = [
  "Xin chào, tôi đang ghi âm để nhân bản giọng nói của mình.",
  "Hôm nay là một ngày đẹp trời để kể một câu chuyện thú vị.",
  "Công nghệ tổng hợp giọng nói ngày càng tự nhiên và sống động.",
];

type Props = {
  active: { refId: string; name: string } | null;
  onCloned: (refId: string, name: string) => void;
  onClear: () => void;
};

export function ClonePanel({ active, onCloned, onClear }: Props) {
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sample, setSample] = useState(0);
  const recRef = useRef<WavRecorder | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const submit = async (blob: Blob, name: string) => {
    setBusy(true);
    setError(null);
    try {
      const { ref_id, frames } = await cloneVoice(blob, name);
      onCloned(ref_id, `${name} (${frames}f)`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggleRecord = async () => {
    setError(null);
    if (!recording) {
      try {
        recRef.current = new WavRecorder();
        await recRef.current.start();
        setRecording(true);
      } catch (e) {
        setError("Không truy cập được micro: " + (e instanceof Error ? e.message : String(e)));
      }
    } else {
      setRecording(false);
      const blob = await recRef.current!.stop();
      recRef.current = null;
      await submit(blob, "recording.wav");
    }
  };

  return (
    <section className="panel clone-panel">
      <label className="field-label">Nhân bản giọng nói</label>

      {active ? (
        <div className="clone-active">
          <span>🎙 Đang dùng giọng nhân bản: <b>{active.name}</b></span>
          <button className="link" onClick={onClear}>× bỏ chọn</button>
        </div>
      ) : (
        <p className="clone-hint">Ghi âm 3–5 giây hoặc tải lên một đoạn audio mẫu.</p>
      )}

      <div className="clone-sample">
        <span className="sample-label">Đọc thử câu này:</span>
        <p className="sample-text">"{SAMPLE_TEXTS[sample]}"</p>
        <button className="link" onClick={() => setSample((s) => (s + 1) % SAMPLE_TEXTS.length)}>
          ↻ câu khác
        </button>
      </div>

      <div className="clone-actions">
        <button
          className={recording ? "rec on" : "rec"}
          onClick={toggleRecord}
          disabled={busy}
        >
          {recording ? "⏺ Dừng & nhân bản" : "⏺ Ghi âm"}
        </button>
        <button className="clone-upload" onClick={() => fileRef.current?.click()} disabled={busy || recording}>
          ⬆ Tải lên
        </button>
        <input
          ref={fileRef}
          type="file"
          accept="audio/*"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void submit(f, f.name);
            e.currentTarget.value = "";
          }}
        />
        {busy && <span className="muted">Đang xử lý…</span>}
        {recording && <span className="rec-dot">● đang ghi</span>}
      </div>
      {error && <p className="error">{error}</p>}
    </section>
  );
}
