import type { Node } from "reactflow";
import type { Graph, NodeSpec, RunStep } from "../../studioApi";

let seq = 0;
export const newId = (t: string) => `${t}-${Date.now().toString(36)}-${seq++}`;

/** Build a node's default config object from its spec's field defaults. */
export function defaultsFromSpec(spec: NodeSpec | undefined): Record<string, unknown> {
  const cfg: Record<string, unknown> = {};
  for (const f of spec?.fields ?? []) {
    if (f.default !== undefined) cfg[f.key] = f.default;
  }
  return cfg;
}

export function nodeStyle(selected: boolean, status?: RunStep["status"]) {
  const color =
    status === "done"
      ? "#3fb950"
      : status === "running"
      ? "#d29922"
      : status === "failed"
      ? "#f85149"
      : "#2f3b52";
  return {
    background: "#1c2333",
    color: "#e6edf3",
    border: `2px solid ${selected ? "#7c83ff" : color}`,
    borderRadius: 10,
    width: 220,
    padding: 8,
    fontSize: 13,
  };
}

export function nodeLabel(specByType: Record<string, NodeSpec>, n: Node): string {
  const t = (n.data as any).nodeType as string;
  const cfg = (n.data as any).config ?? {};
  const label = specByType[t]?.label ?? t;
  if (t === "Source" && cfg.slug) {
    return `${label}\n📖 ${cfg.slug} ${cfg.first ?? 1}–${cfg.last ?? "?"}`;
  }
  return label;
}

/** Convert the live React Flow nodes/edges back into a serializable Graph. */
export function toGraph(
  meta: { id: string; name: string; version: number },
  nodes: Node[],
  edges: { source: string; target: string }[],
): Graph {
  return {
    ...meta,
    nodes: nodes.map((n) => ({
      id: n.id,
      type: (n.data as any).nodeType,
      config: (n.data as any).config ?? {},
      position: n.position,
    })),
    edges: edges.map((e) => ({ from: e.source, to: e.target })),
  };
}
