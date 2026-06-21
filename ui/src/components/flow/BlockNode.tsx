import { Handle, Position, type NodeProps } from "reactflow";

/**
 * One node renderer for the whole palette. The wrapper's border/colour comes
 * from `node.style` (validity / run status). Control blocks (If/Loop) expose
 * multiple named source handles; everything else has a single default output.
 */
export function BlockNode({ data }: NodeProps) {
  const handles: string[] = (data as any).handles ?? [];
  const label: string = (data as any).label ?? "";
  return (
    <>
      <Handle type="target" position={Position.Top} />
      <div style={{ whiteSpace: "pre-wrap", lineHeight: 1.25 }}>{label}</div>
      {handles.length === 0 ? (
        <Handle type="source" position={Position.Bottom} />
      ) : (
        <>
          <div
            style={{
              display: "flex",
              justifyContent: "space-around",
              marginTop: 6,
              fontSize: 10,
              opacity: 0.8,
            }}
          >
            {handles.map((h) => (
              <span key={h}>{h}</span>
            ))}
          </div>
          {handles.map((h, i) => (
            <Handle
              key={h}
              id={h}
              type="source"
              position={Position.Bottom}
              style={{ left: `${((i + 1) / (handles.length + 1)) * 100}%` }}
            />
          ))}
        </>
      )}
    </>
  );
}
