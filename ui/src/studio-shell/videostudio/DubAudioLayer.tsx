import { useEffect, useRef, useState } from "react";
import { clipMediaUrl, type DubClip } from "../../studioApi";

/** Resolve a segment's media URL this many seconds before it starts (prefetch). */
const LOOKAHEAD = 3;

interface Props {
  clips: DubClip[];
  time: number;
  playing: boolean;
  /** Live Vietnamese-track volume (0..1) — applies immediately on inspector drag. */
  volume: number;
}

/**
 * Plays the Vietnamese dub PER SEGMENT from the `dub:tts` clips at their timeline
 * positions, so dragging a segment moves its voice in the preview too. Uses a
 * SINGLE <audio> element whose src swaps to the active segment — WKWebView caps
 * how many media elements can load/play at once, so one element + the preview
 * video is reliable where a per-segment element each was not (later segments
 * silently failed to play). URLs are prefetched + cached so the swap is gapless.
 * Overlapping segments fall back to the later one (the export still mixes both).
 */
export function DubAudioLayer({ clips, time, playing, volume }: Props) {
  const ref = useRef<HTMLAudioElement | null>(null);
  const lastTime = useRef(time);
  const [urls, setUrls] = useState<Record<string, string>>({});

  const tts = clips.filter((c) => c.origin?.startsWith("dub:tts") && c.source);
  // The segment under the playhead (last-starting one wins on overlap).
  const active =
    tts
      .filter((c) => time >= c.start_s && time < c.start_s + c.dur_s)
      .sort((a, b) => a.start_s - b.start_s)
      .pop() ?? null;

  // Prefetch URLs for the active + upcoming segments so the src swap is instant.
  const soon = tts.filter((c) => time >= c.start_s - LOOKAHEAD && time < c.start_s + c.dur_s);
  const soonKey = soon.map((c) => c.id).join(",");
  useEffect(() => {
    let alive = true;
    const missing = soon.filter((c) => !urls[c.id]);
    if (!missing.length) return;
    Promise.all(missing.map((c) => clipMediaUrl(c.id).then((u) => [c.id, u] as const))).then(
      (pairs) => {
        if (alive) setUrls((prev) => ({ ...prev, ...Object.fromEntries(pairs) }));
      },
    );
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [soonKey]);

  const url = active ? urls[active.id] : undefined;

  // Drive the single element. Runs every render; only acts on start/seg-change/seek
  // so smooth playback isn't interrupted.
  useEffect(() => {
    const a = ref.current;
    const jumped = Math.abs(time - lastTime.current) > 0.35;
    lastTime.current = time;
    if (!a) return;
    a.volume = Math.max(0, Math.min(1, volume));
    if (active && playing && url) {
      const want = active.in_s + (time - active.start_s);
      // `src` is bound below; when `active` changes, url changes → the element
      // reloads and `a.paused` flips true, so we (re)start it at `want`.
      if (a.paused || jumped) {
        a.currentTime = Math.max(0, want);
        void a.play().catch(() => {});
      }
    } else if (!a.paused) {
      a.pause();
    }
  });

  return <audio ref={ref} src={url} preload="auto" />;
}
