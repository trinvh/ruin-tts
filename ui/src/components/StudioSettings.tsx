import { useEffect, useState } from "react";
import { getConfig, putConfig, type AppConfig } from "../studioApi";

/// Edits all automation settings (API keys, service URLs, render profile),
/// persisted server-side via /api/config.
export function StudioSettings() {
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [status, setStatus] = useState<string>("");
  const [loadError, setLoadError] = useState(false);

  const load = async () => {
    setLoadError(false);
    setStatus("Đang tải…");
    // The automation server may still be starting up — retry with backoff.
    for (let attempt = 0; attempt < 12; attempt++) {
      try {
        setCfg(await getConfig());
        setStatus("");
        return;
      } catch {
        await new Promise((r) => setTimeout(r, 700));
      }
    }
    setLoadError(true);
    setStatus("");
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!cfg) {
    return (
      <div className="cfg-loading">
        {loadError ? (
          <>
            <p className="muted small">Không kết nối được máy chủ tự động hóa (cổng 8090).</p>
            <button className="mini" onClick={load}>Thử lại</button>
          </>
        ) : (
          <p className="muted small">Đang tải cấu hình tự động hóa…</p>
        )}
      </div>
    );
  }

  const set = (patch: Partial<AppConfig>) => setCfg({ ...cfg, ...patch });
  const setP = (patch: Partial<AppConfig["profile"]>) => setCfg({ ...cfg, profile: { ...cfg.profile, ...patch } });

  const save = async () => {
    setStatus("Đang lưu…");
    try { await putConfig(cfg); setStatus("✓ Đã lưu"); }
    catch (e) { setStatus(String(e)); }
  };

  const T = (v: string | null, on: (s: string) => void, ph = "") => (
    <input className="cfg-in" value={v ?? ""} onChange={(e) => on(e.target.value)} placeholder={ph} spellCheck={false} />
  );
  const N = (v: number, on: (n: number) => void) => (
    <input className="cfg-in num" type="number" value={v} onChange={(e) => on(+e.target.value)} />
  );

  return (
    <div className="cfg">
      <h4>Khóa & dịch vụ</h4>
      <div className="cfg-grid">
        <label>Ruin API key {T(cfg.ruin_key, (v) => set({ ruin_key: v }), "ruin_…")}</label>
        <label>Ruin API base {T(cfg.ruin_base, (v) => set({ ruin_base: v }))}</label>
        <label>VieNeu TTS base {T(cfg.tts_base, (v) => set({ tts_base: v }))}</label>
      </div>

      <h4>YouTube</h4>
      <div className="cfg-grid">
        <label>Client ID {T(cfg.yt_client_id, (v) => set({ yt_client_id: v }))}</label>
        <label>Client secret {T(cfg.yt_client_secret, (v) => set({ yt_client_secret: v }))}</label>
        <label>Refresh token {T(cfg.yt_refresh_token, (v) => set({ yt_refresh_token: v }))}</label>
        <label>Quyền riêng tư
          <select className="cfg-in" value={cfg.yt_privacy} onChange={(e) => set({ yt_privacy: e.target.value })}>
            <option value="private">private</option>
            <option value="unlisted">unlisted</option>
            <option value="public">public</option>
          </select>
        </label>
      </div>

      <h4>Hồ sơ dựng</h4>
      <div className="cfg-grid">
        <label>Tên kênh {T(cfg.profile.site_name, (v) => setP({ site_name: v }))}</label>
        <label>Giọng đọc {T(cfg.profile.voice, (v) => setP({ voice: v }))}</label>
        <label>Định dạng
          <select className="cfg-in" value={cfg.profile.format} onChange={(e) => setP({ format: e.target.value })}>
            <option value="mp3">mp3</option>
            <option value="wav">wav</option>
          </select>
        </label>
        <label>Từ/phút (WPM) {N(cfg.profile.wpm, (n) => setP({ wpm: n }))}</label>
        <label>Giới hạn/video (giây) {N(cfg.profile.cap_seconds, (n) => setP({ cap_seconds: n }))}</label>
        <label>Phụ phí (giây) {N(cfg.profile.overhead_seconds, (n) => setP({ overhead_seconds: n }))}</label>
        <label>Ảnh/video nền {T(cfg.profile.background_path, (v) => setP({ background_path: v || null }), "/đường/dẫn.jpg")}</label>
        <label>Nhạc mở đầu {T(cfg.profile.intro_music_path, (v) => setP({ intro_music_path: v || null }))}</label>
        <label>Nhạc nền {T(cfg.profile.bg_music_path, (v) => setP({ bg_music_path: v || null }))}</label>
      </div>

      <h4>Mẫu văn bản</h4>
      <label className="cfg-area">Lời mở đầu<textarea value={cfg.profile.intro_template} onChange={(e) => setP({ intro_template: e.target.value })} /></label>
      <label className="cfg-area">Lời kết<textarea value={cfg.profile.outro_template} onChange={(e) => setP({ outro_template: e.target.value })} /></label>
      <label className="cfg-area">Tiêu đề<textarea value={cfg.profile.title_template} onChange={(e) => setP({ title_template: e.target.value })} /></label>
      <label className="cfg-area">Mô tả<textarea value={cfg.profile.description_template} onChange={(e) => setP({ description_template: e.target.value })} /></label>
      <label className="cfg-area">Thẻ (tags)<textarea value={cfg.profile.tags_template} onChange={(e) => setP({ tags_template: e.target.value })} /></label>

      <div className="cfg-foot">
        <button className="generate" onClick={save}>Lưu cấu hình</button>
        <span className="muted small">{status}</span>
      </div>
    </div>
  );
}
