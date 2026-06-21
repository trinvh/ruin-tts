import { useCallback, useEffect, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { deleteWorkflow, listWorkflows, type Graph } from "../studioApi";

export function FlowsHome() {
  const navigate = useNavigate();
  const [workflows, setWorkflows] = useState<Graph[] | null>(null);
  const [err, setErr] = useState("");

  const refresh = useCallback(() => {
    // studio-server may still be starting — retry a few times.
    let cancelled = false;
    (async () => {
      for (let i = 0; i < 20 && !cancelled; i++) {
        try {
          const list = await listWorkflows();
          if (!cancelled) {
            setWorkflows(list);
            setErr("");
          }
          return;
        } catch (e) {
          setErr(String(e));
          await new Promise((r) => setTimeout(r, 700));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => refresh(), [refresh]);

  const remove = async (id: string) => {
    if (!confirm("Xoá pipeline này?")) return;
    await deleteWorkflow(id);
    refresh();
  };

  return (
    <div className="mx-auto w-full max-w-5xl">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold text-ink">Pipelines</h2>
          <p className="mt-1 text-sm text-muted">
            Quy trình kéo–thả: Nguồn truyện → Lấy chương → Chia theo thời lượng → Xử lý hậu kỳ → Tải lên.
          </p>
        </div>
        <button
          className="rounded-lg bg-brand px-4 py-2 text-sm font-medium text-white transition hover:brightness-110"
          onClick={() => navigate({ to: "/flows/$id", params: { id: "new" } })}
        >
          ＋ Tạo pipeline
        </button>
      </div>

      {err && workflows === null && (
        <p className="mt-6 rounded-lg border border-border bg-surface p-4 text-sm text-muted">
          Đang kết nối tới máy chủ tự động hóa…
        </p>
      )}

      <div className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {/* Create-new tiles */}
        <button
          onClick={() => navigate({ to: "/flows/$id", params: { id: "new" } })}
          className="flex min-h-[8rem] flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border bg-surface/40 text-muted transition hover:border-brand hover:text-ink"
        >
          <span className="text-3xl leading-none">＋</span>
          <span className="text-sm">Pipeline mới</span>
          <span className="text-[11px] text-muted">5 khối tuyến tính</span>
        </button>
        <button
          onClick={() => navigate({ to: "/flows/$id", params: { id: "new-loop" } })}
          className="flex min-h-[8rem] flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border bg-surface/40 text-muted transition hover:border-brand hover:text-ink"
        >
          <span className="text-3xl leading-none">↻</span>
          <span className="text-sm">Pipeline có vòng lặp</span>
          <span className="text-[11px] text-muted">xử lý từng chunk riêng</span>
        </button>

        {workflows?.map((w) => (
          <div
            key={w.id}
            className="group relative flex min-h-[8rem] flex-col rounded-xl border border-border bg-surface-2 p-4 transition hover:border-brand"
          >
            <Link
              to="/flows/$id"
              params={{ id: w.id }}
              className="flex-1"
            >
              <div className="text-base font-semibold text-ink">{w.name}</div>
              <div className="mt-1 text-xs text-muted">
                {w.nodes.length} khối · v{w.version}
              </div>
              <div className="mt-3 flex flex-wrap gap-1">
                {w.nodes.slice(0, 5).map((n) => (
                  <span
                    key={n.id}
                    className="rounded bg-canvas px-1.5 py-0.5 text-[10px] text-muted"
                  >
                    {n.type}
                  </span>
                ))}
              </div>
            </Link>
            <button
              onClick={() => remove(w.id)}
              className="absolute right-2 top-2 rounded px-2 py-1 text-xs text-muted opacity-0 transition hover:bg-canvas hover:text-fail group-hover:opacity-100"
              title="Xoá"
            >
              🗑
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
