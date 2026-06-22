import { useEffect, useRef, useState } from "react";
import { C } from "../theme";
import { clipMediaUrl, type DubClip, type DubClipGeo } from "../../studioApi";

interface Props {
  clips: DubClip[];
  time: number;
  /** The preview stage element overlays are positioned within. */
  stageRef: React.RefObject<HTMLDivElement | null>;
  playing: boolean;
  onUpdate: (cid: string, geo: DubClipGeo) => void;
}

const clamp01 = (v: number) => Math.max(0, Math.min(1, v));
const active = (c: DubClip, t: number) => t >= c.start_s && t < c.start_s + c.dur_s;
const geoOf = (c: DubClip): DubClipGeo => ({
  track: c.track, start_s: c.start_s, dur_s: c.dur_s, in_s: c.in_s, volume: c.volume,
  x: c.x, y: c.y, w: c.w, opacity: c.opacity, text: c.text, text_style: c.text_style,
});

/**
 * Composites the user-added clips over the preview stage: image + video layers
 * (drag to move, corner to resize), text overlays, and audio elements played in
 * sync. The source video + dub subtitle/banner are rendered by PreviewStage /
 * OverlayLayer; this only handles `origin='user'` clips so the two don't fight.
 */
export function ClipPreview({ clips, time, stageRef, playing, onUpdate }: Props) {
  const userClips = clips.filter((c) => c.origin === "user");
  const visual = userClips.filter((c) => c.kind === "image" || c.kind === "video" || c.kind === "text");
  const audio = userClips.filter((c) => c.kind === "audio");

  const [urls, setUrls] = useState<Record<string, string>>({});
  const [sel, setSel] = useState<string | null>(null);
  const [live, setLive] = useState<Record<string, Partial<DubClipGeo>>>({});
  const drag = useRef<{ cid: string; mode: "move" | "resize"; x: number; y: number; bw: number; bh: number; base: DubClip } | null>(null);

  // Resolve media URLs for clips with a source file.
  useEffect(() => {
    let alive = true;
    const withSrc = userClips.filter((c) => c.source);
    Promise.all(withSrc.map((c) => clipMediaUrl(c.id).then((u) => [c.id, u] as const))).then((pairs) => {
      if (alive) setUrls(Object.fromEntries(pairs));
    });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [userClips.map((c) => c.id).join(",")]);

  // Drag-move / resize for image & video clips.
  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      const dx = (e.clientX - d.x) / d.bw;
      const dy = (e.clientY - d.y) / d.bh;
      setLive((l) =>
        d.mode === "move"
          ? { ...l, [d.cid]: { x: clamp01(d.base.x + dx), y: clamp01(d.base.y + dy) } }
          : { ...l, [d.cid]: { w: Math.max(0.03, Math.min(1, d.base.w + dx)) } },
      );
    };
    const onUp = () => {
      const d = drag.current;
      drag.current = null;
      if (!d) return;
      setLive((l) => {
        const patch = l[d.cid];
        if (patch) onUpdate(d.cid, { ...geoOf(d.base), ...patch });
        const { [d.cid]: _drop, ...rest } = l;
        return rest;
      });
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => { window.removeEventListener("pointermove", onMove); window.removeEventListener("pointerup", onUp); };
  }, [onUpdate]);

  const start = (c: DubClip, mode: "move" | "resize") => (e: React.PointerEvent) => {
    e.stopPropagation();
    e.preventDefault();
    const box = stageRef.current?.getBoundingClientRect();
    if (!box) return;
    setSel(c.id);
    drag.current = { cid: c.id, mode, x: e.clientX, y: e.clientY, bw: box.width, bh: box.height, base: c };
  };

  // Deselect when clicking anywhere outside a clip overlay (so the handles hide).
  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const t = e.target as HTMLElement | null;
      if (!t || !t.closest("[data-clip-ov]")) setSel(null);
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, []);
  // Never show edit handles during playback.
  const showSel = (id: string) => sel === id && !playing;

  return (
    <>
      {/* audio layers (no DOM position) */}
      {audio.map((c) => (
        <AudioClip key={c.id} url={urls[c.id]} clip={c} time={time} playing={playing} />
      ))}

      {/* visual layers */}
      {visual.map((c) => {
        if (!active(c, time)) return null;
        const g = { ...c, ...live[c.id] };
        const selected = showSel(c.id);
        const frame: React.CSSProperties = {
          position: "absolute", left: `${g.x * 100}%`, top: `${g.y * 100}%`,
          width: `${g.w * 100}%`, opacity: c.opacity, touchAction: "none",
          outline: selected ? `1.5px solid ${C.coral}` : "1.5px solid transparent", outlineOffset: 2,
        };
        if (c.kind === "text") {
          return (
            <div key={c.id} data-clip-ov onPointerDown={start(c, "move")} style={{ ...frame, cursor: "move", textAlign: "center" }}>
              <span style={{ fontWeight: 700, color: "#fff", textShadow: "0 1px 4px rgba(0,0,0,.85)", fontSize: 22 }}>{c.text}</span>
              {selected && <ResizeHandle onDown={start(c, "resize")} />}
            </div>
          );
        }
        return (
          <div key={c.id} data-clip-ov onPointerDown={start(c, "move")} style={{ ...frame, cursor: "move" }}>
            {c.kind === "image" && urls[c.id] && (
              <img src={urls[c.id]} alt="" draggable={false} style={{ width: "100%", height: "auto", display: "block", pointerEvents: "none" }} />
            )}
            {c.kind === "video" && urls[c.id] && (
              <VideoClip url={urls[c.id]} clip={c} time={time} playing={playing} />
            )}
            {selected && <ResizeHandle onDown={start(c, "resize")} />}
          </div>
        );
      })}
    </>
  );
}

function ResizeHandle({ onDown }: { onDown: (e: React.PointerEvent) => void }) {
  return (
    <div
      onPointerDown={onDown}
      title="Kéo để phóng to / thu nhỏ"
      style={{ position: "absolute", right: -6, bottom: -6, width: 14, height: 14, background: C.coral, border: "2px solid #fff", borderRadius: 3, cursor: "nwse-resize" }}
    />
  );
}

/** A user video layer: seeks to the right source frame; plays while in range.
 *  Only realigns on start/seek so smooth playback isn't yanked. */
function VideoClip({ url, clip, time, playing }: { url: string; clip: DubClip; time: number; playing: boolean }) {
  const ref = useRef<HTMLVideoElement | null>(null);
  const lastTime = useRef(time);
  useEffect(() => {
    const v = ref.current;
    const jumped = Math.abs(time - lastTime.current) > 0.35;
    lastTime.current = time;
    if (!v) return;
    const want = clip.in_s + (time - clip.start_s);
    if (playing) {
      if (v.paused && !v.ended) {
        v.currentTime = Math.max(0, want);
        void v.play().catch(() => {});
      } else if (jumped) {
        v.currentTime = Math.max(0, want);
        if (v.paused) void v.play().catch(() => {});
      }
    } else if (!v.paused) {
      v.pause();
    }
  }, [time, playing, clip.in_s, clip.start_s]);
  return <video ref={ref} src={url} muted playsInline style={{ width: "100%", height: "auto", display: "block", pointerEvents: "none" }} />;
}

/** A user audio layer: kept in sync with the transport, played only in range.
 *  Only realigns on start/seek (no per-frame currentTime write → no stutter). */
function AudioClip({ url, clip, time, playing }: { url?: string; clip: DubClip; time: number; playing: boolean }) {
  const ref = useRef<HTMLAudioElement | null>(null);
  const lastTime = useRef(time);
  const inRange = active(clip, time);
  useEffect(() => {
    const a = ref.current;
    if (a) a.volume = Math.max(0, Math.min(1, clip.volume));
  }, [clip.volume]);
  useEffect(() => {
    const a = ref.current;
    const jumped = Math.abs(time - lastTime.current) > 0.35;
    lastTime.current = time;
    if (!a) return;
    if (inRange && playing) {
      const want = clip.in_s + (time - clip.start_s);
      if (a.paused && !a.ended) {
        a.currentTime = Math.max(0, want);
        void a.play().catch(() => {});
      } else if (jumped) {
        a.currentTime = Math.max(0, want);
        if (a.paused) void a.play().catch(() => {});
      }
    } else if (!a.paused) {
      a.pause();
    }
  }, [inRange, playing, time, clip.in_s, clip.start_s]);
  if (!url) return null;
  return <audio ref={ref} src={url} preload="auto" />;
}
