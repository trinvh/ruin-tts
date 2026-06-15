import { useEffect, useState } from "react";
import { base } from "../api";

const ENDPOINTS: [string, string][] = [
  ["GET", "/v1/info"],
  ["GET", "/v1/voices"],
  ["POST", "/v1/tts"],
  ["POST", "/v1/clone"],
  ["POST", "/v1/jobs"],
  ["GET", "/v1/jobs/:id"],
  ["DELETE", "/v1/jobs/:id"],
  ["GET", "/v1/jobs/:id/download"],
];

export function ApiInfo() {
  const [url, setUrl] = useState("");
  useEffect(() => {
    base().then(setUrl);
  }, []);

  return (
    <section className="panel api">
      <div className="api-head">
        <label className="field-label">API máy chủ</label>
        <code className="api-url" title="Base URL">{url || "…"}</code>
      </div>
      <p className="api-note">
        Giao diện này gọi trực tiếp các endpoint HTTP bên dưới — bạn có thể tích hợp
        từ ứng dụng khác theo cùng cách.
      </p>
      <ul className="api-list">
        {ENDPOINTS.map(([m, p]) => (
          <li key={m + p}>
            <span className={`verb ${m.toLowerCase()}`}>{m}</span>
            <code>{p}</code>
          </li>
        ))}
      </ul>
      <details className="api-curl">
        <summary>Ví dụ curl</summary>
        <pre>{`curl -X POST ${url}/v1/tts \\
  -H 'content-type: application/json' \\
  -d '{"text":"Xin chào","voice":"Bình An","format":"mp3"}' \\
  -o out.mp3`}</pre>
      </details>
    </section>
  );
}
