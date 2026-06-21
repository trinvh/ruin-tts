import { useEffect, useState } from "react";
import { updateDubSettings, type DubProject, type DubSettings } from "../../studioApi";
import { settingsOf } from "./shared";

/** Modal holding all per-project dubbing settings, so they don't crowd the page. */
export function SettingsDialog({
  project,
  open,
  onClose,
  onSaved,
}: {
  project: DubProject;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [draft, setDraft] = useState<DubSettings>(settingsOf(project));

  useEffect(() => {
    if (open) setDraft(settingsOf(project));
  }, [open, project]);

  if (!open) return null;

  const set = (over: Partial<DubSettings>) => setDraft((d) => ({ ...d, ...over }));
  const save = async () => {
    await updateDubSettings(project.id, draft);
    onSaved();
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onClick={onClose}>
      <div
        className="max-h-[85vh] w-full max-w-lg overflow-auto rounded-xl border border-border bg-canvas shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <h3 className="text-sm font-semibold text-ink">Cấu hình dự án</h3>
          <button className="text-muted hover:text-ink" onClick={onClose}>✕</button>
        </div>

        <div className="space-y-5 px-5 py-4 text-sm">
          <Group title="Chung">
            <Row label="Tên dự án">
              <input
                className="w-full rounded border border-border bg-surface-2 px-2 py-1 text-ink"
                value={draft.name}
                onChange={(e) => set({ name: e.target.value })}
              />
            </Row>
            <Row label="Gemini model">
              <input
                className="w-full rounded border border-border bg-surface-2 px-2 py-1 text-ink"
                value={draft.gemini_model}
                onChange={(e) => set({ gemini_model: e.target.value })}
              />
            </Row>
          </Group>

          <Group title="Lồng tiếng">
            <Slider label="Âm gốc khi nghe thử" value={draft.original_volume} min={0} max={1} step={0.05}
              fmt={(v) => `${Math.round(v * 100)}%`} onChange={(v) => set({ original_volume: v })} />
            <Slider label="Tốc độ đọc tối đa" value={draft.speed_cap} min={1} max={2} step={0.05}
              fmt={(v) => `${v.toFixed(2)}×`} onChange={(v) => set({ speed_cap: v })}
              hint="Giữ ~1.5 cho tự nhiên; cao hơn dễ chói." />
          </Group>

          <Group title="Khi xuất video">
            <Check label="Ghi phụ đề tiếng Việt vào video" checked={draft.burn_subtitles}
              onChange={(v) => set({ burn_subtitles: v })} />
            <Check label="Làm mờ che phụ đề gốc" checked={draft.blur_subtitle}
              onChange={(v) => set({ blur_subtitle: v })} />
            <p className="text-xs text-muted">
              Vị trí phụ đề và vùng mờ chỉnh bằng cách kéo trực tiếp trên khung nghe thử.
            </p>
          </Group>
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <button className="rounded-md px-3 py-1.5 text-sm text-muted hover:text-ink" onClick={onClose}>Huỷ</button>
          <button className="rounded-md bg-brand px-4 py-1.5 text-sm font-medium text-white hover:opacity-90" onClick={save}>
            Lưu
          </button>
        </div>
      </div>
    </div>
  );
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">{title}</div>
      <div className="space-y-3">{children}</div>
    </div>
  );
}
function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs text-muted">{label}</span>
      {children}
    </label>
  );
}
function Check({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex items-center gap-2">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="text-ink">{label}</span>
    </label>
  );
}
function Slider({
  label, value, min, max, step, fmt, onChange, hint,
}: {
  label: string; value: number; min: number; max: number; step: number;
  fmt: (v: number) => string; onChange: (v: number) => void; hint?: string;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between text-xs">
        <span className="text-muted">{label}</span>
        <b className="text-ink">{fmt(value)}</b>
      </div>
      <input type="range" className="w-full" min={min} max={max} step={step} value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))} />
      {hint && <p className="mt-0.5 text-[11px] text-muted/80">{hint}</p>}
    </div>
  );
}
