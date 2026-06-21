import { useCallback, useEffect, useState } from "react";
import { C, FONT } from "./theme";
import { Icon } from "./icons";
import { HoverBox } from "./ui";
import {
  downloadFfmpeg,
  ffmpegStatus,
  mediaAiBase,
  serverBase,
  studioBase,
  type FfmpegStatus,
} from "../platform";

type Step = "checking" | "ffmpeg" | "models" | "done";

async function ping(url: string): Promise<boolean> {
  try {
    return (await fetch(url, { cache: "no-store" })).ok;
  } catch {
    return false;
  }
}

/** First-launch setup: ensures ffmpeg is present (offers a download) and waits
 *  for the sidecar servers to finish pulling their models on first run. */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState<Step>("checking");
  const [ff, setFf] = useState<FfmpegStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState({ tts: false, studio: false, mediaAi: false });

  const checkFfmpeg = useCallback(async () => {
    const s = await ffmpegStatus();
    setFf(s);
    setStep(!s || s.available ? "models" : "ffmpeg");
  }, []);

  useEffect(() => {
    void checkFfmpeg();
  }, [checkFfmpeg]);

  // Poll server/model readiness while on the "models" step.
  useEffect(() => {
    if (step !== "models") return;
    let alive = true;
    const tick = async () => {
      const [t, s, m] = await Promise.all([serverBase(), studioBase(), mediaAiBase()]);
      const [tts, studio, mediaAi] = await Promise.all([
        t ? ping(`${t}/health`) : Promise.resolve(false),
        s ? ping(`${s}/health`) : Promise.resolve(false),
        m ? ping(`${m}/health`) : Promise.resolve(false),
      ]);
      if (!alive) return;
      setReady({ tts, studio, mediaAi });
      if (tts && studio && mediaAi) {
        setStep("done");
        setTimeout(onDone, 800);
      }
    };
    void tick();
    const h = setInterval(tick, 2500);
    return () => {
      alive = false;
      clearInterval(h);
    };
  }, [step, onDone]);

  const onDownload = async () => {
    setError(null);
    setDownloading(true);
    try {
      await downloadFfmpeg();
      await checkFfmpeg();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setDownloading(false);
    }
  };

  return (
    <div className="bss" style={{ position: "fixed", inset: 0, zIndex: 1000, background: C.appBg, color: "#fff", fontFamily: FONT, display: "grid", placeItems: "center" }}>
      <div style={{ width: 460, maxWidth: "90vw", background: C.card, border: `1px solid ${C.borderSoft}`, borderRadius: 16, padding: 32, boxShadow: "0 20px 60px rgba(0,0,0,.45)" }}>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", textAlign: "center", marginBottom: 24 }}>
          <div style={{ width: 48, height: 48, borderRadius: "50%", background: "conic-gradient(from 180deg,#9288E0 0 50%,#2d2a44 50% 100%)", border: "2px solid #6f64c4", marginBottom: 14, boxShadow: "0 6px 24px rgba(146,136,224,.3)" }} />
          <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700, letterSpacing: "-.01em" }}>Chuẩn bị Beesoft Studio</h1>
          <p style={{ margin: "8px 0 0", fontSize: 13, color: C.muted, lineHeight: 1.5 }}>Thiết lập lần đầu — tải công cụ &amp; model cần thiết.</p>
        </div>

        {step === "checking" && <Center>Đang kiểm tra…</Center>}

        {step === "ffmpeg" && (
          <div>
            <Row icon="film" title="FFmpeg" sub={ff?.downloadable ? "Cần cho việc xử lý & xuất video — chưa tìm thấy trên máy." : "Không tìm thấy — hãy cài ffmpeg và thêm vào PATH."} state={downloading ? "running" : "todo"} />
            {error && <div style={{ fontSize: 11.5, color: C.pink, margin: "10px 2px 0" }}>{error}</div>}
            <div style={{ display: "flex", gap: 10, marginTop: 18 }}>
              {ff?.downloadable && (
                <button onClick={() => void onDownload()} disabled={downloading} style={btn(C.purple, downloading)}>
                  {downloading ? "Đang tải ffmpeg…" : "Tải ffmpeg"}
                </button>
              )}
              <button onClick={() => void checkFfmpeg()} disabled={downloading} style={btn(C.panel2, downloading)}>Kiểm tra lại</button>
            </div>
          </div>
        )}

        {step === "models" && (
          <div>
            <p style={{ fontSize: 12, color: C.muted, margin: "0 0 14px" }}>Đang tải model (lần đầu có thể vài GB) — giữ ứng dụng mở.</p>
            <Row icon="wave" title="Giọng đọc (TTS)" sub="vieneu-server" state={ready.tts ? "done" : "running"} />
            <Row icon="film" title="Phân tích & lồng tiếng" sub="studio-server" state={ready.studio ? "done" : "running"} />
            <Row icon="runs" title="ASR + diarization" sub="media-ai" state={ready.mediaAi ? "done" : "running"} />
          </div>
        )}

        {step === "done" && <Center>Sẵn sàng! 🎉</Center>}

        <div style={{ textAlign: "center", marginTop: 22 }}>
          <button onClick={onDone} style={{ border: "none", background: "transparent", color: C.muted3, fontSize: 11.5, cursor: "pointer", fontFamily: FONT, textDecoration: "underline" }}>Bỏ qua &amp; vào ứng dụng</button>
        </div>
      </div>
    </div>
  );
}

function btn(bg: string, disabled: boolean): React.CSSProperties {
  return { flex: 1, height: 38, border: "none", background: bg, color: "#fff", borderRadius: 9, cursor: disabled ? "default" : "pointer", opacity: disabled ? 0.6 : 1, fontFamily: FONT, fontSize: 13, fontWeight: 600 };
}

function Center({ children }: { children: React.ReactNode }) {
  return <div style={{ textAlign: "center", padding: "24px 0", color: C.muted, fontSize: 14 }}>{children}</div>;
}

function Row({ icon, title, sub, state }: { icon: Parameters<typeof Icon>[0]["name"]; title: string; sub: string; state: "todo" | "running" | "done" }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "10px 12px", background: C.panel2, border: `1px solid ${C.borderSoft}`, borderRadius: 10, marginBottom: 8 }}>
      <div style={{ width: 32, height: 32, flex: "none", borderRadius: 8, background: C.panel3, color: C.purpleLt, display: "grid", placeItems: "center" }}>
        <Icon name={icon} size={17} stroke={1.7} />
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 600 }}>{title}</div>
        <div style={{ fontSize: 10.5, color: C.muted3, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{sub}</div>
      </div>
      {state === "done" ? (
        <span style={{ color: C.teal }}><Icon name="check" size={16} stroke={2.4} /></span>
      ) : state === "running" ? (
        <span style={{ width: 14, height: 14, border: `2px solid ${C.purpleLt}`, borderTopColor: "transparent", borderRadius: "50%", animation: "bss-spin .7s linear infinite", display: "inline-block" }} />
      ) : (
        <HoverBox style={{ width: 8, height: 8, borderRadius: "50%", background: C.muted4 }} />
      )}
    </div>
  );
}
