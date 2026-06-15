import type { QueueItem } from "../queue";

type Props = {
  items: QueueItem[];
  paused: boolean;
  setPaused: (p: boolean) => void;
  cancel: (id: string) => void;
  cancelAll: () => void;
  clearFinished: () => void;
  stats: { queued: number; running: number; done: number };
  onPlay: (item: QueueItem) => void;
  onSaveAs: (item: QueueItem) => void;
  onReveal: (item: QueueItem) => void;
  canReveal: boolean;
};

const STATUS_LABEL: Record<QueueItem["status"], string> = {
  queued: "chờ",
  running: "đang tạo",
  done: "xong",
  failed: "lỗi",
  cancelled: "đã hủy",
};

export function QueuePanel(p: Props) {
  const active = p.stats.queued + p.stats.running > 0;
  return (
    <section className="panel queue">
      <div className="queue-head">
        <label className="field-label">Hàng đợi tạo audio</label>
        <div className="queue-stats">
          <span>{p.stats.running} đang chạy</span>
          <span>·</span>
          <span>{p.stats.queued} chờ</span>
          <span>·</span>
          <span>{p.stats.done} xong</span>
        </div>
      </div>

      <div className="queue-controls">
        <button
          className="qc"
          disabled={!active}
          onClick={() => p.setPaused(!p.paused)}
        >
          {p.paused ? "▶ Tiếp tục" : "⏸ Tạm dừng"}
        </button>
        <button className="qc" disabled={!active} onClick={p.cancelAll}>
          ⏹ Hủy tất cả
        </button>
        <button className="qc" onClick={p.clearFinished}>
          🧹 Xóa mục đã xong
        </button>
      </div>

      {p.items.length === 0 ? (
        <div className="empty">Hàng đợi trống. Nhấn “Tạo giọng nói”.</div>
      ) : (
        <ul className="qlist">
          {p.items.map((it) => (
            <li key={it.id} className={`qitem ${it.status}`}>
              <div className="qmain">
                <span className={`qbadge ${it.status}`}>{STATUS_LABEL[it.status]}</span>
                <div className="qtext">
                  <span className="qlabel">{it.label || "—"}</span>
                  <span className="qsub">
                    {it.voiceLabel} · {it.format.toUpperCase()}
                    {it.durationS != null ? ` · ${it.durationS.toFixed(1)}s` : ""}
                    {it.savedPath ? " · đã lưu" : ""}
                    {it.error ? ` · ${it.error}` : ""}
                  </span>
                </div>
              </div>
              <div className="qacts">
                {it.status === "running" && <span className="spinner" />}
                {it.status === "done" && (
                  <>
                    <button className="qbtn" title="Phát" onClick={() => p.onPlay(it)}>▶</button>
                    <button className="qbtn" title="Lưu thành…" onClick={() => p.onSaveAs(it)}>↓</button>
                    {p.canReveal && it.savedPath && (
                      <button className="qbtn" title="Mở thư mục" onClick={() => p.onReveal(it)}>📂</button>
                    )}
                  </>
                )}
                {(it.status === "queued" || it.status === "running") && (
                  <button className="qbtn danger" title="Hủy" onClick={() => p.cancel(it.id)}>✕</button>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
