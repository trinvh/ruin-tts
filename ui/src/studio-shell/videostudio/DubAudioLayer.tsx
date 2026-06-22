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
  const inRange = time >= clip.start_s && time < clip.start_s + clip.dur_s;
  useEffect(() => {
    const a = ref.current;
    if (a) a.volume = Math.max(0, Math.min(1, volume));
  }, [volume]);
  useEffect(() => {
    const a = ref.current;
    if (!a) return;
    if (inRange && playing) {
      const want = clip.in_s + (time - clip.start_s);
      // Only (re)align on start / seek — never write currentTime mid-playback,
      // which would re-buffer and cause stutter/dropouts.
      const needSeek = Math.abs(a.currentTime - want) > 0.3;
      if (needSeek) a.currentTime = Math.max(0, want);
      // Don't restart a clip that already finished (audio shorter than the slot).
      if (a.paused && (!a.ended || needSeek)) void a.play().catch(() => {});
    } else if (!a.paused) {
      a.pause();
    }
    // `time` is a dep so we react to seeks; the body no-ops during smooth play.
  }, [inRange, playing, time, clip.in_s, clip.start_s]);
  if (!url) return null;
  return <audio ref={ref} src={url} preload="auto" />;
}
