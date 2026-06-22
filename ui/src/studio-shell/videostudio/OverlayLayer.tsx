import { useEffect, useRef, useState } from "react";
import { C } from "../theme";
import { overlayImageUrl, type DubOverlay, type DubOverlayGeo } from "../../studioApi";

interface Props {
  overlays: DubOverlay[];
  time: number;
  /** The preview stage element overlays are positioned within. */
  stageRef: React.RefObject<HTMLDivElement | null>;
  onUpdate: (oid: string, geo: DubOverlayGeo) => void;
  onDelete: (oid: string) => void;
}

const geoOf = (o: DubOverlay): DubOverlayGeo => ({
  start_s: o.start_s, end_s: o.end_s, x: o.x, y: o.y, w: o.w, opacity: o.opacity,
});

const clamp01 = (v: number) => Math.max(0, Math.min(1, v));

/** Banner/image overlays over the preview: drag to move, drag the corner to resize. */
export function OverlayLayer({ overlays, time, stageRef, onUpdate, onDelete }: Props) {
  const [urls, setUrls] = useState<Record<string, string>>({});
  const [sel, setSel] = useState<string | null>(null);
  // Live geometry while dragging (committed to the server on pointer up).
  const [live, setLive] = useState<Record<string, Partial<DubOverlayGeo>>>({});
  const drag = useRef<{ oid: string; mode: "move" | "resize"; x: number; y: number; bw: number; bh: number; base: DubOverlay } | null>(null);

  useEffect(() => {
    let alive = true;
    Promise.all(overlays.map((o) => overlayImageUrl(o.id).then((u) => [o.id, u] as const))).then((pairs) => {
      if (alive) setUrls(Object.fromEntries(pairs));
    });
    return () => { alive = false; };
  }, [overlays]);

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      const dx = (e.clientX - d.x) / d.bw;
      const dy = (e.clientY - d.y) / d.bh;
      setLive((l) => {
        if (d.mode === "move") {
          return { ...l, [d.oid]: { x: clamp01(d.base.x + dx), y: clamp01(d.base.y + dy) } };
        }
        return { ...l, [d.oid]: { w: Math.max(0.03, Math.min(1, d.base.w + dx)) } };
      });
    };
    const onUp = () => {
      const d = drag.current;
      drag.current = null;
      if (!d) return;
      setLive((l) => {
        const patch = l[d.oid];
        if (patch) onUpdate(d.oid, { ...geoOf(d.base), ...patch });
        const { [d.oid]: _, ...rest } = l;
        return rest;
      });
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => { window.removeEventListener("pointermove", onMove); window.removeEventListener("pointerup", onUp); };
  }, [onUpdate]);

  const start = (o: DubOverlay, mode: "move" | "resize") => (e: React.PointerEvent) => {
    e.stopPropagation();
    e.preventDefault();
    const box = stageRef.current?.getBoundingClientRect();
    if (!box) return;
    setSel(o.id);
    drag.current = { oid: o.id, mode, x: e.clientX, y: e.clientY, bw: box.width, bh: box.height, base: o };
  };

  return (
    <>
      {overlays.map((o) => {
        const visible = o.end_s > o.start_s ? time >= o.start_s && time < o.end_s : true;
        if (!visible) return null;
        const g = { ...o, ...live[o.id] };
        const selected = sel === o.id;
        return (
          <div
            key={o.id}
            onPointerDown={start(o, "move")}
            style={{
              position: "absolute", left: `${g.x * 100}%`, top: `${g.y * 100}%`, width: `${g.w * 100}%`,
              opacity: o.opacity, cursor: "move", touchAction: "none",
              outline: selected ? `1.5px solid ${C.coral}` : "1.5px solid transparent",
              outlineOffset: 2,
            }}
          >
            {urls[o.id] && <img src={urls[o.id]} alt="" draggable={false} style={{ width: "100%", height: "auto", display: "block", pointerEvents: "none" }} />}
            {selected && (
              <>
                <div
                  onPointerDown={start(o, "resize")}
                  title="Kéo để phóng to / thu nhỏ"
                  style={{ position: "absolute", right: -6, bottom: -6, width: 14, height: 14, background: C.coral, border: "2px solid #fff", borderRadius: 3, cursor: "nwse-resize" }}
                />
                <button
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={(e) => { e.stopPropagation(); onDelete(o.id); }}
                  title="Xoá banner"
                  style={{ position: "absolute", top: -10, right: -10, width: 20, height: 20, borderRadius: "50%", border: "2px solid #fff", background: C.coral, color: "#fff", cursor: "pointer", fontSize: 12, lineHeight: 1, display: "grid", placeItems: "center" }}
                >×</button>
              </>
            )}
          </div>
        );
      })}
    </>
  );
}
