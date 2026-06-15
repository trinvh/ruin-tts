import { useEffect, useState } from "react";
import { fileUrl, retryRun, type RunDetail, type RunStep } from "../../studioApi";

export function RunDetailView({ detail }: { detail: RunDetail }) {
  const [openStep, setOpenStep] = useState<string | null>(null);
  return (
    <>
      <div className="rd-head">
        <b>{detail.label}</b>{" "}
        <span className={`badge ${detail.status}`}>{detail.status}</span>
      </div>
      {detail.error && <p className="error">{detail.error}</p>}
      <ul className="steps">
        {detail.steps.map((s) => (
          <li key={s.node_id} className={`step ${s.status}`}>
            <div
              className="step-row"
              onClick={() => setOpenStep(openStep === s.node_id ? null : s.node_id)}
            >
              <span className={`dot ${s.status}`} />
              <span className="step-name">{s.node_type}</span>
              <span className="step-status">{s.status}</span>
              <button
                className="step-retry"
                title="Chạy lại từ khối này"
                onClick={(e) => {
                  e.stopPropagation();
                  retryRun(detail.id, s.node_id).catch((err) => alert(String(err)));
                }}
              >
                ↻
              </button>
            </div>
            {openStep === s.node_id && <StepIO step={s} />}
          </li>
        ))}
      </ul>
    </>
  );
}

function StepIO({ step }: { step: RunStep }) {
  const logs = step.output?.logs ?? [];
  const state = step.output?.state ?? {};
  const media = collectMedia(state);
  return (
    <div className="step-io">
      {step.input != null && Object.keys(step.input as object).length > 0 && (
        <div className="io-block">
          <span className="io-label">Đầu vào</span>
          <pre>{JSON.stringify(step.input, null, 2)}</pre>
        </div>
      )}
      {logs.length > 0 && (
        <div className="io-block">
          <span className="io-label">Nhật ký</span>
          <pre>{logs.join("\n")}</pre>
        </div>
      )}
      {media.map((m) => (
        <MediaView key={m.path} label={m.label} path={m.path} />
      ))}
      {Object.keys(state).length > 0 && (
        <details className="io-block">
          <summary>Trạng thái (JSON)</summary>
          <pre>{JSON.stringify(state, null, 2)}</pre>
        </details>
      )}
    </div>
  );
}

function MediaView({ label, path }: { label: string; path: string }) {
  const [url, setUrl] = useState("");
  useEffect(() => {
    fileUrl(path).then(setUrl);
  }, [path]);
  const isVideo = /\.mp4$/i.test(path);
  return (
    <div className="io-block media">
      <span className="io-label">{label}</span>
      <code className="mpath">{path}</code>
      {url &&
        (isVideo ? (
          <video src={url} controls className="mplayer" />
        ) : (
          <audio src={url} controls className="mplayer" />
        ))}
    </div>
  );
}

function collectMedia(obj: unknown, label = ""): { label: string; path: string }[] {
  const out: { label: string; path: string }[] = [];
  const walk = (v: unknown, key: string) => {
    if (typeof v === "string" && /\.(mp3|wav|mp4|m4a)$/i.test(v)) out.push({ label: key, path: v });
    else if (Array.isArray(v)) v.forEach((x, i) => walk(x, `${key}[${i}]`));
    else if (v && typeof v === "object")
      for (const [k, val] of Object.entries(v)) walk(val, key ? `${key}.${k}` : k);
  };
  walk(obj, label);
  return out;
}
