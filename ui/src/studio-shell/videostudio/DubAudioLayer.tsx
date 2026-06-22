import { useEffect, useRef, useState } from "react";
import { clipMediaUrl, type DubClip } from "../../studioApi";

/** Mount each segment's audio this many seconds before it starts (preload). */
const LOOKAHEAD = 1.5;

interface Props {
  clips: DubClip[];
  time: number;
  playing: boolean;
  /** Live Vietnamese-track volume (0..1) — applies immediately on inspector drag. */
  volume: number;
}

/**
 * Plays the Vietnamese dub PER SEGMENT, straight from the `dub:tts` clips at
 * their timeline positions, instead of one pre-merged track. Dragging a segment
 * on the timeline therefore moves its voice in the preview too (the merged track
 * couldn't reflect that without a rebuild). Only clips near the playhead are
 * mounted, so a long video doesn't spin up hundreds of <audio> elements.
 */
export function DubAudioLayer({ clips, time, playing, volume }: Props) {
  const tts = clips.filter((c) => c.origin?.startsWith("dub:tts") && c.source);
  const near = tts.filter(
    (c) => time >= c.start_s - LOOKAHEAD && time < c.start_s + c.dur_s + 0.15,
  );

  const [urls, setUrls] = useState<Record<string, string>>({});
  const key = near.map((c) => c.id).join(",");
  useEffect(() => {
    let alive = true;
    Promise.all(near.map((c) => clipMediaUrl(c.id).then((u) => [c.id, u] as const))).then(
      (pairs) => {
        if (alive) setUrls((prev) => ({ ...prev, ...Object.fromEntries(pairs) }));
      },
    );
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return (
    <>
      {near.map((c) => (
        <TtsClip key={c.id} url={urls[c.id]} clip={c} time={time} playing={playing} volume={volume} />
      ))}
    </>
  );
}

function TtsClip({
  url,
  clip,
  time,
  playing,
  volume,
}: {
  url?: string;
  clip: DubClip;
  time: number;
  playing: boolean;
  volume: number;
}) {
  const ref = useRef<HTMLAudioElement | null>(null);
  const lastTime = useRef(time);
  const inRange = time >= clip.start_s && time < clip.start_s + clip.dur_s;
  useEffect(() => {
    const a = ref.current;
    if (a) a.volume = Math.max(0, Math.min(1, volume));
  }, [volume]);
  useEffect(() => {
    const a = ref.current;
    // A real SEEK = the timeline jumped (not the ~16ms/frame of smooth playback).
    const jumped = Math.abs(time - lastTime.current) > 0.35;
    lastTime.current = time;
    if (!a) return;
    if (inRange && playing) {
      const want = clip.in_s + (time - clip.start_s);
      if (a.paused && !a.ended) {
        // start: align once, then let it play freely
        a.currentTime = Math.max(0, want);
        void a.play().catch(() => {});
      } else if (jumped) {
        // seek: realign (also un-pauses a finished/paused clip if back in range)
        a.currentTime = Math.max(0, want);
        if (a.paused) void a.play().catch(() => {});
      }
      // else: playing smoothly → never touch currentTime (no stutter/dropout)
    } else if (!a.paused) {
      a.pause();
    }
  }, [inRange, playing, time, clip.in_s, clip.start_s]);
  if (!url) return null;
  return <audio ref={ref} src={url} preload="auto" />;
}
