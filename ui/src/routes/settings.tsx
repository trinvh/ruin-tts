import { isTauri, pickDirectory } from "../platform";
import { useTtsSettings } from "../ttsSettings";
import { Help } from "../components/Help";
import { StudioSettings } from "../components/StudioSettings";

export function SettingsPage() {
  const { outputDir, setOutputDir, concurrency, setConcurrency } = useTtsSettings();

  const browseDir = async () => {
    const d = await pickDirectory();
    if (d) setOutputDir(d);
  };

  return (
    <div className="mx-auto w-full max-w-3xl">
      <h2 className="text-2xl font-semibold text-ink">Cài đặt</h2>
      <p className="mt-1 text-sm text-muted">Tùy chọn đọc văn bản và cấu hình tự động hóa.</p>

      <section className="panel settings mt-5">
        <label className="field-label">Đọc văn bản (TTS)</label>
        <div className="setting-row">
          <span className="setting-name">Thư mục lưu</span>
          <code className="setting-val" title={outputDir}>
            {outputDir || "—"}
          </code>
          <button className="mini" onClick={browseDir} disabled={!isTauri()}>
            Đổi…
          </button>
        </div>
        {!isTauri() && <p className="muted small">Đổi thư mục lưu chỉ khả dụng trong ứng dụng desktop.</p>}
        <div className="setting-row">
          <span className="setting-name">
            Số luồng song song
            <Help title="Số luồng song song">
              Số audio được tạo cùng lúc. Tăng để xử lý nhiều chương nhanh hơn, nhưng máy chủ chỉ chạy
              hiệu quả tới số worker được cấu hình.
            </Help>
          </span>
          <input
            className="num"
            type="number"
            min={1}
            max={8}
            value={concurrency}
            onChange={(e) => setConcurrency(Number(e.target.value))}
          />
        </div>
      </section>

      <section className="panel mt-4">
        <label className="field-label">Cấu hình tự động hóa (Ruin / YouTube / dựng video)</label>
        <StudioSettings />
      </section>
    </div>
  );
}
