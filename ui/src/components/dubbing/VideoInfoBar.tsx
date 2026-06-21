import { useEffect, useState } from "react";
import { getDubInfo, type DubMediaInfo } from "../../studioApi";
import { fmtBytes, fmtDuration } from "./shared";

/** Collapsible technical details of the source video (duration, codec, …). */
export function VideoInfoBar({ id, videoPath }: { id: string; videoPath: string }) {
  const [info, setInfo] = useState<DubMediaInfo | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let alive = true;
    getDubInfo(id)
      .then((i) => alive && setInfo(i))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [id]);

  const v = info?.video;
  const a = info?.audio;
  const summary = info
    ? [
        v ? `${v.width}×${v.height}` : null,
        info.duration ? fmtDuration(info.duration) : null,
        v?.codec?.toUpperCase(),
        fmtBytes(info.size),
      ]
        .filter(Boolean)
        .join(" · ")
    : "Đang đọc thông tin…";

  return (
    <div className="rounded-lg border border-border bg-surface-2/50">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-xs text-muted hover:text-ink"
      >
        <span className="truncate">
          <span className="text-ink/70">Thông tin video</span> · {summary}
        </span>
        <span className={`shrink-0 transition-transform ${open ? "rotate-90" : ""}`}>›</span>
      </button>
      {open && (
        <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 border-t border-border px-3 py-2.5 text-xs sm:grid-cols-4">
          <Field label="Đường dẫn" value={videoPath} wide />
          <Field label="Thời lượng" value={fmtDuration(info?.duration ?? null)} />
          <Field label="Dung lượng" value={fmtBytes(info?.size ?? null)} />
          <Field label="Định dạng" value={info?.format_name ?? "—"} />
          <Field label="Độ phân giải" value={v ? `${v.width}×${v.height}` : "—"} />
          <Field label="Video codec" value={v ? `${v.codec ?? "?"}${v.profile ? ` (${v.profile})` : ""}` : "—"} />
          <Field label="FPS" value={v?.fps ? `${v.fps}` : "—"} />
          <Field label="Pixel" value={v?.pix_fmt ?? "—"} />
          <Field label="Audio codec" value={a?.codec ?? "—"} />
          <Field
            label="Âm thanh"
            value={a ? `${a.channels ?? "?"}ch · ${a.sample_rate ? `${Math.round(+a.sample_rate / 1000)}kHz` : "?"}` : "—"}
          />
        </div>
      )}
    </div>
  );
}

function Field({ label, value, wide }: { label: string; value: string; wide?: boolean }) {
  return (
    <div className={wide ? "col-span-2 sm:col-span-4" : ""}>
      <div className="text-[10px] uppercase tracking-wide text-muted/70">{label}</div>
      <div className="truncate text-ink/90" title={value}>
        {value}
      </div>
    </div>
  );
}
