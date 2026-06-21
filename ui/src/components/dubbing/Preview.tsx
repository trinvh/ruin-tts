import { useCallback, useEffect, useRef, useState } from "react";
import { dubVideoUrl, fileUrl, updateDubSettings, type DubProject, type DubSegment } from "../../studioApi";
import { clock, genderLabel, settingsOf, speakerName } from "./shared";

type Rect = { x: number; y: number; w: number; h: number };

/** Original video + Vietnamese track in sync, with demo toggles (voice / subtitles
 *  / cover overlay), drag-to-place blur + subtitle, and a synced transcript on the
 *  right that highlights and auto-scrolls to the current line. */
export function Preview({
  project,
  segments,
  genderBySpeaker,
  mediaVer,
  onSaved,
}: {
  project: DubProject;
  segments: DubSegment[];
  genderBySpeaker: Record<string, string | null>;
  /** Bumped only when the VN track is rebuilt, so settings edits don't reload media. */
  mediaVer: number;
  onSaved: () => void;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const vnRef = useRef<HTMLAudioElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const [videoUrl, setVideoUrl] = useState("");
  const [vnUrl, setVnUrl] = useState("");
  const [vol, setVol] = useState(project.original_volume);
  const [showVn, setShowVn] = useState(true);
  const [showSubs, setShowSubs] = useState(false);
  const [showBlur, setShowBlur] = useState(project.blur_subtitle);
  const [editRegion, setEditRegion] = useState(false);
  const [editSub, setEditSub] = useState(false);
  const [drag, setDrag] = useState<Rect | null>(null);
  const [dragSubY, setDragSubY] = useState<number | null>(null);
  const [time, setTime] = useState(0);
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const subYRef = useRef(project.sub_y);

  // The source video never changes → load once, never cache-bust (otherwise the
  // src changes on every settings save and the player reloads to a black frame).
  useEffect(() => {
    void dubVideoUrl(project.id).then(setVideoUrl);
  }, [project.id]);
  // The VN track only changes when it's rebuilt → bust by mediaVer, not updated_at.
  useEffect(() => {
    if (project.vn_track_path) void fileUrl(project.vn_track_path).then((u) => setVnUrl(`${u}&v=${mediaVer}`));
  }, [project.vn_track_path, mediaVer]);

  useEffect(() => {
    if (videoRef.current) videoRef.current.volume = vol;
  }, [vol, videoUrl]);
  useEffect(() => {
    if (vnRef.current) vnRef.current.muted = !showVn;
  }, [showVn, vnUrl]);

  const onTime = useCallback(() => {
    const v = videoRef.current, a = vnRef.current;
    if (!v) return;
    setTime(v.currentTime);
    if (a && Math.abs(a.currentTime - v.currentTime) > 0.3) a.currentTime = v.currentTime;
  }, []);
  const seek = (t: number) => {
    if (videoRef.current) videoRef.current.currentTime = t;
  };

  const frac = (e: React.PointerEvent) => {
    const r = boxRef.current!.getBoundingClientRect();
    return {
      x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)),
      y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)),
    };
  };
  const onDown = (e: React.PointerEvent) => {
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const p = frac(e);
    if (editSub) {
      subYRef.current = p.y;
      setDragSubY(p.y);
    } else if (editRegion) {
      startRef.current = p;
      setDrag({ x: p.x, y: p.y, w: 0, h: 0 });
    }
  };
  const onMove = (e: React.PointerEvent) => {
    const p = frac(e);
    if (editSub && dragSubY !== null) {
      subYRef.current = p.y;
      setDragSubY(p.y);
    } else if (editRegion && startRef.current) {
      const s = startRef.current;
      setDrag({ x: Math.min(s.x, p.x), y: Math.min(s.y, p.y), w: Math.abs(p.x - s.x), h: Math.abs(p.y - s.y) });
    }
  };
  const onUp = () => {
    if (editSub && dragSubY !== null) {
      const y = subYRef.current;
      setDragSubY(null);
      void updateDubSettings(project.id, settingsOf(project, { sub_y: y })).then(() => {
        setShowSubs(true);
        onSaved();
      });
      return;
    }
    if (editRegion && drag) {
      startRef.current = null;
      const r = drag;
      setDrag(null);
      if (r.w < 0.02 || r.h < 0.02) return;
      void updateDubSettings(
        project.id,
        settingsOf(project, { blur_subtitle: true, blur_x: r.x, blur_y: r.y, blur_w: r.w, blur_h: r.h }),
      ).then(() => {
        setShowBlur(true);
        onSaved();
      });
    }
  };

  const region: Rect = drag ?? { x: project.blur_x, y: project.blur_y, w: project.blur_w, h: project.blur_h };
  const subLine = dragSubY ?? project.sub_y;
  const editing = editRegion || editSub;
  const activeSeg = segments.find((s) => time >= s.start_s && time < s.end_s);
  const subText = activeSeg?.text_vi?.trim() || (editSub ? "Phụ đề tiếng Việt" : "");

  return (
    <div className="grid gap-4 lg:grid-cols-[1fr_340px]">
      <div>
        <div className="mb-2 flex flex-wrap gap-1.5">
          <Toggle on={showVn} set={setShowVn} label="Tiếng Việt" />
          <Toggle on={showSubs} set={setShowSubs} label="Phụ đề" />
          <Toggle on={showBlur} set={setShowBlur} label="Lớp phủ che" />
          <span className="mx-1 w-px self-stretch bg-border" />
          <Toggle on={editRegion} set={(b) => { setEditRegion(b); setEditSub(false); if (b) setShowBlur(true); }} label="✛ Vùng mờ" />
          <Toggle on={editSub} set={(b) => { setEditSub(b); setEditRegion(false); if (b) setShowSubs(true); }} label="✛ Vị trí phụ đề" />
        </div>

        <div className="flex justify-center overflow-hidden rounded-lg border border-border bg-black">
          <div ref={boxRef} className="relative inline-block leading-none">
            <video
              ref={videoRef}
              src={videoUrl}
              controls={!editing}
              className="block max-h-[460px]"
              onPlay={() => vnRef.current?.play().catch(() => {})}
              onPause={() => vnRef.current?.pause()}
              onSeeking={onTime}
              onSeeked={onTime}
              onTimeUpdate={onTime}
            />

            {/* Blur cover — feathered at the edges via a radial mask. The brand
                border (edit mode) shows the exact rectangle. */}
            {(showBlur || editRegion) && (
              <>
                <div
                  className="pointer-events-none absolute"
                  style={{
                    left: `${region.x * 100}%`, top: `${region.y * 100}%`,
                    width: `${region.w * 100}%`, height: `${region.h * 100}%`,
                    backdropFilter: "blur(8px)", WebkitBackdropFilter: "blur(8px)",
                    background: "rgba(10,10,10,0.30)",
                    maskImage: "radial-gradient(ellipse 100% 100% at center, #000 55%, transparent 100%)",
                    WebkitMaskImage: "radial-gradient(ellipse 100% 100% at center, #000 55%, transparent 100%)",
                  }}
                />
                {editRegion && (
                  <div
                    className="pointer-events-none absolute border-2 border-dashed border-brand"
                    style={{ left: `${region.x * 100}%`, top: `${region.y * 100}%`, width: `${region.w * 100}%`, height: `${region.h * 100}%` }}
                  />
                )}
              </>
            )}

            {/* Vietnamese subtitle — custom overlay so it sits ABOVE the blur and
                exactly at the chosen position (z-10). */}
            {(showSubs || editSub) && subText && (
              <div
                className="pointer-events-none absolute left-0 right-0 z-10 flex justify-center px-[5%]"
                style={{ top: `${subLine * 100}%`, transform: "translateY(-50%)" }}
              >
                <span
                  className="text-center font-medium leading-snug text-white"
                  style={{
                    fontSize: "clamp(12px, 2.6vw, 22px)",
                    textShadow: "0 0 4px rgba(0,0,0,0.95), 0 1px 4px rgba(0,0,0,0.95)",
                    outline: editSub ? "1px dashed var(--color-brand, #6aa3ff)" : "none",
                  }}
                >
                  {subText}
                </span>
              </div>
            )}

            {editing && (
              <div className={`absolute inset-0 z-20 ${editSub ? "cursor-ns-resize" : "cursor-crosshair"}`}
                onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} />
            )}
          </div>
        </div>
        <audio ref={vnRef} src={vnUrl} preload="auto" />

        <div className="mt-2 flex flex-wrap items-center gap-4 text-sm">
          <label className="flex items-center gap-2">
            <span className="text-muted">Âm lượng gốc</span>
            <input type="range" min={0} max={1} step={0.05} value={vol} onChange={(e) => setVol(parseFloat(e.target.value))} />
            <b className="w-12 text-ink">{Math.round(vol * 100)}%</b>
          </label>
          {editRegion && <span className="text-xs text-amber-300">Kéo một khung che lên video, rồi tắt nút.</span>}
          {editSub && <span className="text-xs text-amber-300">Kéo lên/xuống để đặt vị trí phụ đề, rồi tắt nút.</span>}
        </div>
      </div>

      <SyncedTranscript segments={segments} genderBySpeaker={genderBySpeaker} time={time} onSeek={seek} />
    </div>
  );
}

/** Read-only transcript that highlights the line at `time` and auto-scrolls to it. */
function SyncedTranscript({
  segments,
  genderBySpeaker,
  time,
  onSeek,
}: {
  segments: DubSegment[];
  genderBySpeaker: Record<string, string | null>;
  time: number;
  onSeek: (t: number) => void;
}) {
  const activeIdx = segments.findIndex((s) => time >= s.start_s && time < s.end_s);
  const listRef = useRef<HTMLDivElement>(null);
  const activeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeIdx]);

  return (
    <div className="flex max-h-[560px] flex-col rounded-lg border border-border bg-surface-2/40">
      <div className="border-b border-border px-3 py-2 text-xs font-semibold text-muted">
        Câu thoại · đang phát theo timeline
      </div>
      <div ref={listRef} className="flex-1 overflow-auto">
        {segments.map((s, i) => {
          const active = i === activeIdx;
          return (
            <button
              key={s.id}
              ref={active ? activeRef : undefined}
              onClick={() => onSeek(s.start_s + 0.01)}
              className={`block w-full border-b border-border/60 px-3 py-2 text-left transition ${
                active ? "bg-brand/15" : "hover:bg-surface-2"
              }`}
            >
              <div className="flex items-center justify-between text-[10px] text-muted">
                <span>{clock(s.start_s)} · {speakerName(s.speaker)} · {genderLabel(genderBySpeaker[s.speaker])}</span>
                {s.factor && s.factor > 1.01 && <span>{s.factor.toFixed(2)}×</span>}
              </div>
              <div className={`text-sm ${active ? "text-ink" : "text-ink/80"}`}>
                {s.text_vi || <span className="text-muted italic">(chưa dịch)</span>}
              </div>
              <div className="truncate text-[11px] text-muted/70">{s.text_src}</div>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function Toggle({ on, set, label }: { on: boolean; set: (b: boolean) => void; label: string }) {
  return (
    <button
      onClick={() => set(!on)}
      className={`rounded-md px-2.5 py-1 text-xs transition ${on ? "bg-brand text-white" : "bg-surface-2 text-muted hover:text-ink"}`}
    >
      {on ? "● " : "○ "}
      {label}
    </button>
  );
}
