import { useEffect, useState } from "react";
import { getRun, listRuns, type RunDetail, type RunSummary } from "../studioApi";
import { RunDetailView } from "../components/flow/RunDetail";

export function RunsPage() {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [detail, setDetail] = useState<RunDetail | null>(null);

  useEffect(() => {
    const tick = () =>
      listRuns()
        .then((r) => {
          setRuns(r);
          setActive((a) => a ?? r[0]?.id ?? null);
        })
        .catch(() => {});
    tick();
    const h = setInterval(tick, 2500);
    return () => clearInterval(h);
  }, []);

  useEffect(() => {
    if (!active) return;
    let alive = true;
    const tick = async () => {
      try {
        const d = await getRun(active);
        if (alive) setDetail(d);
      } catch {
        /* ignore */
      }
    };
    tick();
    const h = setInterval(tick, 1200);
    return () => {
      alive = false;
      clearInterval(h);
    };
  }, [active]);

  return (
    <div className="mx-auto w-full max-w-5xl">
      <h2 className="text-2xl font-semibold text-ink">Lịch sử chạy</h2>
      <p className="mt-1 text-sm text-muted">Hàng đợi và tiến trình theo thời gian thực của từng khối.</p>

      <div className="mt-5 grid grid-cols-[16rem_1fr] gap-4">
        <ul className="max-h-[70vh] space-y-1 overflow-y-auto rounded-xl border border-border bg-surface p-2">
          {runs.length === 0 && <li className="p-2 text-sm text-muted">Chưa có run nào.</li>}
          {runs.map((r) => (
            <li
              key={r.id}
              onClick={() => setActive(r.id)}
              className={`flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-sm transition ${
                r.id === active ? "bg-surface-2 text-ink" : "text-muted hover:bg-surface-2/60"
              }`}
            >
              <span className={`dot ${r.status}`} />
              <span className="truncate">{r.label || r.id.slice(0, 8)}</span>
            </li>
          ))}
        </ul>

        <div className="runs-detail max-h-[70vh] overflow-y-auto rounded-xl border border-border bg-surface p-4">
          {detail ? <RunDetailView detail={detail} /> : <p className="muted small">Chọn một run.</p>}
        </div>
      </div>
    </div>
  );
}
