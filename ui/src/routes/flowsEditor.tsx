import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import ReactFlow, {
  addEdge,
  Background,
  Controls,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Connection,
} from "reactflow";
import "reactflow/dist/style.css";
import {
  createRun,
  deleteWorkflow,
  getDefaultGraph,
  getLoopGraph,
  getNodeSpecs,
  getRun,
  getWorkflow,
  listRuns,
  saveWorkflow,
  type Graph,
  type NodeSpec,
  type RunDetail,
  type RunStep,
} from "../studioApi";
import { Inspector } from "../components/flow/Inspector";
import { RunDetailView } from "../components/flow/RunDetail";
import { BlockNode } from "../components/flow/BlockNode";
import {
  defaultsFromSpec,
  newId,
  nodeLabel,
  nodeStyle,
  nodeValidity,
  toGraph,
} from "../components/flow/nodes";

const NODE_TYPES = { block: BlockNode };

export function FlowsEditor() {
  return (
    <ReactFlowProvider>
      <EditorInner />
    </ReactFlowProvider>
  );
}

function EditorInner() {
  const { id } = useParams({ from: "/flows/$id" });
  const navigate = useNavigate();
  const [specs, setSpecs] = useState<NodeSpec[]>([]);
  const [nodes, setNodes, onNodesChange] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [meta, setMeta] = useState({ id: "default", name: "Pipeline", version: 1 });
  const [status, setStatus] = useState("");
  const [drawer, setDrawer] = useState(false);
  const [activeRun, setActiveRun] = useState<string | null>(null);
  const [runStatus, setRunStatus] = useState<Record<string, RunStep["status"]>>({});
  const wrap = useRef<HTMLDivElement>(null);
  // WKWebView (Tauri) drops custom dataTransfer MIME types, so the dragged node
  // type is carried in a ref instead of relying on dataTransfer alone.
  const dragType = useRef<string | null>(null);
  const rf = useReactFlow();

  const applyGraph = useCallback(
    (g: Graph) => {
      setMeta({ id: g.id, name: g.name, version: g.version });
      setNodes(
        g.nodes.map((n, i) => ({
          id: n.id,
          position: n.position ?? { x: 80 + i * 240, y: 120 },
          data: { nodeType: n.type, config: n.config ?? {} },
          type: "block",
          style: nodeStyle(false),
        })),
      );
      setEdges(
        g.edges.map((e, i) => ({
          id: `e${i}`,
          source: e.from,
          target: e.to,
          sourceHandle: e.handle ?? null,
          animated: true,
        })),
      );
    },
    [setNodes, setEdges],
  );

  // Load specs + the requested graph (retry while the server warms up).
  useEffect(() => {
    let alive = true;
    (async () => {
      for (let i = 0; i < 20 && alive; i++) {
        try {
          const s = await getNodeSpecs();
          if (!alive) return;
          setSpecs(s);
          if (id === "new" || id === "new-loop") {
            const tpl = id === "new-loop" ? await getLoopGraph() : await getDefaultGraph();
            if (!alive) return;
            applyGraph({
              ...tpl,
              id: `wf-${Date.now().toString(36)}`,
              name: id === "new-loop" ? "Pipeline có vòng lặp" : "Pipeline mới",
            });
          } else {
            const g = await getWorkflow(id);
            if (!alive) return;
            applyGraph(g);
          }
          return;
        } catch {
          await new Promise((r) => setTimeout(r, 700));
        }
      }
    })();
    return () => {
      alive = false;
    };
  }, [id, applyGraph]);

  const specByType = useMemo(() => Object.fromEntries(specs.map((s) => [s.type, s])), [specs]);

  const onConnect = useCallback(
    (c: Connection) => setEdges((es) => addEdge({ ...c, animated: true }, es)),
    [setEdges],
  );

  const onDrop = useCallback(
    (ev: React.DragEvent) => {
      ev.preventDefault();
      const type = dragType.current || ev.dataTransfer.getData("text/plain");
      dragType.current = null;
      if (!type || !wrap.current) return;
      const b = wrap.current.getBoundingClientRect();
      const position = rf.project({ x: ev.clientX - b.left, y: ev.clientY - b.top });
      const nid = newId(type);
      setNodes((ns) =>
        ns.concat({
          id: nid,
          position,
          data: { nodeType: type, config: defaultsFromSpec(specByType[type]) },
          type: "block",
          style: nodeStyle(false),
        }),
      );
    },
    [rf, setNodes, specByType],
  );

  // Fallback: clicking a palette block also adds it (DnD can be flaky in some
  // webviews). Drops it near the canvas centre.
  const addNode = useCallback(
    (type: string) => {
      const b = wrap.current?.getBoundingClientRect();
      const position = b
        ? rf.project({ x: b.width / 2, y: b.height / 2 })
        : { x: 160, y: 120 };
      setNodes((ns) =>
        ns.concat({
          id: newId(type),
          position,
          data: { nodeType: type, config: defaultsFromSpec(specByType[type]) },
          type: "block",
          style: nodeStyle(false),
        }),
      );
    },
    [rf, setNodes, specByType],
  );

  const graph = useCallback(() => toGraph(meta, nodes, edges), [meta, nodes, edges]);

  const save = useCallback(async () => {
    setStatus("Đang lưu…");
    try {
      await saveWorkflow(graph());
      setStatus("✓ Đã lưu");
      if (id === "new") navigate({ to: "/flows/$id", params: { id: meta.id }, replace: true });
    } catch (e) {
      setStatus(String(e));
    }
  }, [graph, id, meta.id, navigate]);

  const remove = useCallback(async () => {
    if (!confirm("Xoá pipeline này?")) return;
    await deleteWorkflow(meta.id).catch(() => {});
    navigate({ to: "/flows" });
  }, [meta.id, navigate]);

  const updateConfig = useCallback(
    (nodeId: string, config: Record<string, unknown>) => {
      setNodes((ns) => ns.map((n) => (n.id === nodeId ? { ...n, data: { ...n.data, config } } : n)));
    },
    [setNodes],
  );

  const run = useCallback(
    async (preview: boolean) => {
      setStatus(preview ? "Đang tạo xem trước…" : "Đang thêm vào hàng đợi…");
      try {
        const { run_id } = await createRun(graph(), preview);
        setActiveRun(run_id);
        setDrawer(true);
        setStatus("");
      } catch (e) {
        setStatus(String(e));
      }
    },
    [graph],
  );

  const onRunSteps = useCallback((steps: RunStep[]) => {
    setRunStatus(Object.fromEntries(steps.map((s) => [s.node_id, s.status])));
  }, []);

  const selNode = nodes.find((n) => n.id === selected);
  const selSpec = selNode ? specByType[(selNode.data as any).nodeType] : undefined;

  // Styled view of nodes: border reflects run status, else config validity.
  const styledNodes = useMemo(
    () =>
      nodes.map((n) => {
        const t = (n.data as any).nodeType as string;
        const cfg = (n.data as any).config as Record<string, unknown>;
        return {
          ...n,
          data: { ...n.data, label: nodeLabel(specByType, n), handles: specByType[t]?.handles ?? [] },
          style: nodeStyle(n.id === selected, runStatus[n.id], nodeValidity(t, cfg, specByType)),
        };
      }),
    [nodes, selected, runStatus, specByType],
  );

  const invalidCount = useMemo(
    () =>
      nodes.filter(
        (n) => nodeValidity((n.data as any).nodeType, (n.data as any).config, specByType) !== "ok",
      ).length,
    [nodes, specByType],
  );

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex flex-wrap items-center gap-2 border-b border-border pb-3">
        <Link to="/flows" className="rounded-md px-2 py-1.5 text-sm text-muted hover:text-ink">
          ← Pipelines
        </Link>
        <input
          className="min-w-[12rem] flex-1 rounded-md border border-border bg-surface px-3 py-1.5 text-sm text-ink outline-none focus:border-brand"
          value={meta.name}
          onChange={(e) => setMeta({ ...meta, name: e.target.value })}
          title="Tên pipeline"
        />
        <button className="rounded-md border border-border px-3 py-1.5 text-sm text-ink hover:border-brand" onClick={save}>
          💾 Lưu
        </button>
        <button className="rounded-md border border-border px-3 py-1.5 text-sm text-muted hover:text-fail" onClick={remove}>
          🗑
        </button>
        <span className="mx-1 h-5 w-px bg-border" />
        <button
          className={`rounded-md border px-3 py-1.5 text-sm ${drawer ? "border-brand text-ink" : "border-border text-ink hover:border-brand"}`}
          onClick={() => setDrawer((o) => !o)}
        >
          🕒 Tiến trình
        </button>
        <button className="rounded-md border border-border px-3 py-1.5 text-sm text-ink hover:border-brand" onClick={() => run(true)}>
          ⚡ Xem trước
        </button>
        <button
          className="rounded-md bg-brand px-3 py-1.5 text-sm font-medium text-white hover:brightness-110"
          onClick={() => run(false)}
        >
          ＋ Thêm vào hàng đợi
        </button>
      </div>
      {(status || invalidCount > 0) && (
        <div className="flex items-center gap-3 px-1 py-1.5 text-xs">
          {status && <span className="text-muted">{status}</span>}
          {invalidCount > 0 && (
            <span className="text-[#d29922]">
              ⚠ {invalidCount} khối cần cấu hình hoặc không còn hỗ trợ — sửa trước khi chạy.
            </span>
          )}
        </div>
      )}

      {/* Body: palette · canvas · (inspector only when a node is selected) */}
      <div
        className="grid min-h-0 flex-1 overflow-hidden"
        style={{ gridTemplateColumns: selNode && selSpec ? "10rem 1fr 19rem" : "10rem 1fr" }}
      >
        <aside className="overflow-y-auto border-r border-border p-2">
          <div className="px-1 pb-2 text-[11px] font-semibold uppercase tracking-wide text-muted">Khối</div>
          {specs.map((s) => (
            <div
              key={s.type}
              className="mb-1.5 flex cursor-grab select-none items-center justify-between gap-1 rounded-lg border border-border bg-surface-2 px-2 py-1.5 text-[13px] leading-tight text-ink transition hover:border-brand hover:bg-surface active:cursor-grabbing"
              draggable
              onDragStart={(e) => {
                dragType.current = s.type;
                e.dataTransfer.setData("text/plain", s.type);
                e.dataTransfer.effectAllowed = "copy";
              }}
              onDragEnd={() => {
                dragType.current = null;
              }}
              onDoubleClick={() => addNode(s.type)}
              title={s.desc ? `${s.desc}\n(kéo vào canvas hoặc nhấn +)` : "Kéo vào canvas hoặc nhấn +"}
            >
              <span className="truncate">{s.label}</span>
              <button
                className="shrink-0 rounded px-1.5 text-base leading-none text-muted hover:text-ink"
                title="Thêm vào canvas"
                onClick={() => addNode(s.type)}
              >
                ＋
              </button>
            </div>
          ))}

          <div className="mt-3 space-y-1 border-t border-border pt-2 text-[10px] text-muted">
            <div className="flex items-center gap-1.5">
              <span className="inline-block h-2.5 w-2.5 rounded border-2 border-[#d29922]" />
              Cần cấu hình
            </div>
            <div className="flex items-center gap-1.5">
              <span className="inline-block h-2.5 w-2.5 rounded border-2 border-dashed border-[#f85149]" />
              Khối lỗi / không còn hỗ trợ
            </div>
          </div>
        </aside>

        <div
          className="relative h-full bg-canvas"
          ref={wrap}
          onDrop={onDrop}
          onDragOver={(e) => {
            e.preventDefault();
            e.dataTransfer.dropEffect = "copy";
          }}
        >
          <ReactFlow
            nodes={styledNodes}
            edges={edges}
            nodeTypes={NODE_TYPES}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeClick={(_, n) => setSelected(n.id)}
            onPaneClick={() => setSelected(null)}
            proOptions={{ hideAttribution: true }}
            fitView
          >
            <Background color="#30363d" />
            <Controls />
          </ReactFlow>
          {nodes.length === 0 && (
            <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
              <p className="rounded-lg border border-dashed border-border px-4 py-3 text-sm text-muted">
                Kéo khối từ trái vào đây để bắt đầu.
              </p>
            </div>
          )}
        </div>

        {selNode && selSpec && (
          <aside className="overflow-y-auto border-l border-border p-3">
            <Inspector
              node={selNode}
              spec={selSpec}
              onChange={(cfg) => updateConfig(selNode.id, cfg)}
              onDelete={() => {
                setNodes((ns) => ns.filter((x) => x.id !== selNode.id));
                setEdges((es) => es.filter((e) => e.source !== selNode.id && e.target !== selNode.id));
                setSelected(null);
              }}
            />
          </aside>
        )}
      </div>

      {drawer && (
        <RunDrawer
          activeRun={activeRun}
          setActiveRun={setActiveRun}
          onSteps={onRunSteps}
          onClose={() => setDrawer(false)}
        />
      )}
    </div>
  );
}

function RunDrawer({
  activeRun,
  setActiveRun,
  onSteps,
  onClose,
}: {
  activeRun: string | null;
  setActiveRun: (id: string) => void;
  onSteps: (s: RunStep[]) => void;
  onClose: () => void;
}) {
  const [runs, setRuns] = useState<{ id: string; status: string; label: string }[]>([]);
  const [detail, setDetail] = useState<RunDetail | null>(null);

  useEffect(() => {
    const tick = () => listRuns().then(setRuns).catch(() => {});
    tick();
    const h = setInterval(tick, 2500);
    return () => clearInterval(h);
  }, []);

  useEffect(() => {
    if (!activeRun) return;
    let alive = true;
    const tick = async () => {
      try {
        const d = await getRun(activeRun);
        if (!alive) return;
        setDetail(d);
        onSteps(d.steps);
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
  }, [activeRun, onSteps]);

  return (
    <div className="runs">
      <div className="runs-top">
        <strong>Tiến trình</strong>
        <button className="mini" onClick={onClose}>
          Đóng
        </button>
      </div>
      <div className="runs-cols">
        <ul className="runs-list">
          {runs.map((r) => (
            <li key={r.id} className={r.id === activeRun ? "on" : ""} onClick={() => setActiveRun(r.id)}>
              <span className={`dot ${r.status}`} />
              <span className="rl-label">{r.label || r.id.slice(0, 8)}</span>
            </li>
          ))}
        </ul>
        <div className="runs-detail">
          {!detail ? <p className="muted small">Chọn một run.</p> : <RunDetailView detail={detail} />}
        </div>
      </div>
    </div>
  );
}
