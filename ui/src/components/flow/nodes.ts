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

export type Validity = "ok" | "needs-config" | "unknown";

/** Static validation of a node's config (independent of any run). */
export function nodeValidity(
  type: string,
  config: Record<string, unknown> | undefined,
  specByType: Record<string, NodeSpec>,
): Validity {
  const spec = specByType[type];
  if (!spec) return "unknown"; // removed/unknown block type — cannot run
  for (const f of spec.fields) {
    if (f.kind === "novel") {
      const v = config?.[f.key];
      if (!v || (typeof v === "string" && v.trim() === "")) return "needs-config";
    }
  }
  return "ok";
}

export function nodeStyle(
  selected: boolean,
  status?: RunStep["status"],
  validity: Validity = "ok",
) {
  // Run status takes precedence; otherwise reflect config validity.
  let color = "#2f3b52";
  let dashed = false;
  if (status === "done") color = "#3fb950";
  else if (status === "running") color = "#d29922";
  else if (status === "failed") color = "#f85149";
  else if (validity === "unknown") {
    color = "#f85149";
    dashed = true;
  } else if (validity === "needs-config") color = "#d29922";

  return {
    background: "#1c2333",
    color: "#e6edf3",
    border: `2px ${dashed ? "dashed" : "solid"} ${selected ? "#7c83ff" : color}`,
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
  edges: { source: string; target: string; sourceHandle?: string | null }[],
): Graph {
  return {
    ...meta,
    nodes: nodes.map((n) => ({
      id: n.id,
      type: (n.data as any).nodeType,
      config: (n.data as any).config ?? {},
      position: n.position,
    })),
    edges: edges.map((e) => ({
      from: e.source,
      to: e.target,
      handle: e.sourceHandle ?? undefined,
    })),
  };
}
