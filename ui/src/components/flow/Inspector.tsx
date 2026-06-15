import { useState } from "react";
import type { Node } from "reactflow";
import { searchNovels, type NodeSpec, type Novel } from "../../studioApi";

export function Inspector({
  node,
  spec,
  onChange,
  onDelete,
}: {
  node: Node;
  spec: NodeSpec;
  onChange: (c: Record<string, unknown>) => void;
  onDelete: () => void;
}) {
  const cfg = (node.data as any).config as Record<string, unknown>;
  const set = (k: string, v: unknown) => onChange({ ...cfg, [k]: v });
  return (
    <div className="insp">
      <div className="insp-head">
        <strong>{spec.label}</strong>
        <button className="mini danger" onClick={onDelete}>
          Xóa
        </button>
      </div>
      {spec.desc && <p className="muted small">{spec.desc}</p>}
      {spec.fields.length === 0 && <p className="muted small">Khối này không có tham số.</p>}
      {spec.fields.map((f) => (
        <label key={f.key} className="insp-field">
          <span>{f.label}</span>
          {f.kind === "novel" ? (
            <NovelPicker value={(cfg[f.key] as string) ?? ""} onPick={(slug) => set(f.key, slug)} />
          ) : f.kind === "number" ? (
            <input
              className="cfg-in"
              type="number"
              value={(cfg[f.key] as number) ?? ""}
              onChange={(e) => set(f.key, e.target.value === "" ? undefined : Number(e.target.value))}
            />
          ) : f.kind === "bool" ? (
            <input type="checkbox" checked={!!cfg[f.key]} onChange={(e) => set(f.key, e.target.checked)} />
          ) : f.kind === "textarea" ? (
            <textarea
              className="cfg-in"
              value={(cfg[f.key] as string) ?? ""}
              onChange={(e) => set(f.key, e.target.value)}
            />
          ) : f.kind === "select" ? (
            <select
              className="cfg-in"
              value={(cfg[f.key] as string) ?? f.options?.[0]}
              onChange={(e) => set(f.key, e.target.value)}
            >
              {f.options?.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </select>
          ) : (
            <input
              className="cfg-in"
              value={(cfg[f.key] as string) ?? ""}
              onChange={(e) => set(f.key, e.target.value)}
            />
          )}
        </label>
      ))}
    </div>
  );
}

function NovelPicker({ value, onPick }: { value: string; onPick: (slug: string) => void }) {
  const [q, setQ] = useState("");
  const [items, setItems] = useState<Novel[]>([]);
  const [open, setOpen] = useState(false);
  const search = async () => {
    try {
      setItems((await searchNovels(q)).items);
      setOpen(true);
    } catch {
      /* ignore */
    }
  };
  return (
    <div className="novelpick">
      {value && <div className="picked">📖 {value}</div>}
      <div className="np-row">
        <input
          className="cfg-in"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Tìm truyện…"
          onKeyDown={(e) => e.key === "Enter" && search()}
        />
        <button className="mini" onClick={search}>
          Tìm
        </button>
      </div>
      {open && (
        <ul className="np-list">
          {items.map((n) => (
            <li
              key={n.id}
              onClick={() => {
                onPick(n.slug);
                setOpen(false);
              }}
            >
              <b>{n.title}</b> <span>{n.chapterCount} ch</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
