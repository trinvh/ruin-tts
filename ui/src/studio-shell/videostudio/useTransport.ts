import { useCallback, useRef, useState } from "react";

export interface Transport {
  attachVideo: (el: HTMLVideoElement | null) => void;
  attachVn: (el: HTMLAudioElement | null) => void;
  time: number;
  playing: boolean;
  duration: number;
  seek: (t: number) => void;
  togglePlay: () => void;
  toStart: () => void;
  /** mute/unmute the Vietnamese track */
  setVnMuted: (m: boolean) => void;
  /** Vietnamese-track volume 0..1 (muted when 0) */
  setVnVolume: (v: number) => void;
  /** original-track volume 0..1 */
  setOrigVolume: (v: number) => void;
  /** Video lead-in: the source video starts at this timeline second. */
  setVideoOffset: (s: number) => void;
  /** Source video's own duration (seconds), for visibility/clamping. */
  setVideoDur: (s: number) => void;
  /** Total timeline length the playhead runs to. */
  setDuration: (s: number) => void;
  // <video> event handlers (the clock is independent now; these only feed config)
  onTime: () => void;
  onPlay: () => void;
  onPause: () => void;
  onLoaded: () => void;
}

/**
 * Drives the preview off an INDEPENDENT timeline clock (requestAnimationFrame),
 * not the source video's own time. The `<video>` (original) and `<audio>`
 * (Vietnamese track) are slaved to the clock: the video is offset by its lead-in
 * (`videoOffset`) so dragging it right delays it, and every layer — including
 * ClipPreview's user clips — reads the same `time`. This makes the preview match
 * the compositing export instead of being pinned to the video's frame 0.
 */
export function useTransport(): Transport {
  const video = useRef<HTMLVideoElement | null>(null);
  const vn = useRef<HTMLAudioElement | null>(null);
  const [time, setTimeState] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);

  const timeRef = useRef(0);
  const playingRef = useRef(false);
  const durRef = useRef(0);
  const raf = useRef<number | null>(null);
  const lastTs = useRef<number | null>(null);
  const cfg = useRef({ videoOffset: 0, videoDur: 0, origVol: 1, vnVol: 1, vnMuted: false });

  const setTime = (t: number) => {
    timeRef.current = t;
    setTimeState(t);
  };

  // Slave the <video> to the clock. `allowSeek` is true only for jumps (play
  // start, seek); during smooth playback we DON'T touch currentTime — the video
  // plays on its own real clock (which then drives the playhead in `tick`), so
  // there's no per-frame yank that would stutter the picture or its audio.
  const drive = useCallback((t: number, play: boolean, allowSeek: boolean) => {
    const v = video.current;
    const { videoOffset, videoDur } = cfg.current;
    if (!v) return;
    const want = t - videoOffset;
    const inRange = want >= -0.001 && (videoDur <= 0 || want <= videoDur + 0.05);
    if (inRange) {
      if (allowSeek && Math.abs(v.currentTime - want) > 0.2) v.currentTime = Math.max(0, want);
      if (play && v.paused) void v.play().catch(() => {});
      if (!play && !v.paused) v.pause();
    } else {
      if (!v.paused) v.pause();
      if (allowSeek && want < 0 && v.currentTime !== 0) v.currentTime = 0;
    }
  }, []);

  const stop = useCallback(() => {
    playingRef.current = false;
    setPlaying(false);
    if (raf.current != null) cancelAnimationFrame(raf.current);
    raf.current = null;
    lastTs.current = null;
    drive(timeRef.current, false, true);
  }, [drive]);

  const tick = useCallback(
    (ts: number) => {
      if (lastTs.current == null) lastTs.current = ts;
      const dt = (ts - lastTs.current) / 1000;
      lastTs.current = ts;
      const v = video.current;
      const { videoOffset, videoDur } = cfg.current;
      // When the video is the active layer and actually playing, use ITS clock
      // (a real media clock — smooth and drift-free); otherwise integrate (during
      // the lead-in gap or past the video's end).
      const prevWant = timeRef.current - videoOffset;
      const videoActive =
        !!v && !v.paused && prevWant >= -0.05 && (videoDur <= 0 || prevWant <= videoDur);
      let t = videoActive && v ? v.currentTime + videoOffset : timeRef.current + dt;
      const dur = durRef.current;
      if (dur > 0 && t >= dur) {
        setTime(dur);
        drive(dur, false, true);
        playingRef.current = false;
        setPlaying(false);
        raf.current = null;
        lastTs.current = null;
        return;
      }
      setTime(t);
      drive(t, true, false); // play/pause only — never yank currentTime mid-play
      raf.current = requestAnimationFrame(tick);
    },
    [drive],
  );

  const play = useCallback(() => {
    if (playingRef.current) return;
    // restart from 0 if parked at the end
    if (durRef.current > 0 && timeRef.current >= durRef.current - 0.01) setTime(0);
    playingRef.current = true;
    setPlaying(true);
    lastTs.current = null;
    drive(timeRef.current, true, true);
    raf.current = requestAnimationFrame(tick);
  }, [drive, tick]);

  const togglePlay = useCallback(() => {
    if (playingRef.current) stop();
    else play();
  }, [play, stop]);

  const seek = useCallback(
    (t: number) => {
      const clamped = Math.max(0, t);
      setTime(clamped);
      drive(clamped, playingRef.current, true);
    },
    [drive],
  );
  const toStart = useCallback(() => seek(0), [seek]);

  const onLoaded = useCallback(() => {
    const v = video.current;
    if (v && Number.isFinite(v.duration) && v.duration > 0) {
      cfg.current.videoDur = v.duration;
      v.volume = cfg.current.origVol;
    }
  }, []);
  const noop = useCallback(() => {}, []);

  return {
    attachVideo: (el) => {
      video.current = el;
    },
    attachVn: (el) => {
      vn.current = el;
    },
    time,
    playing,
    duration,
    seek,
    togglePlay,
    toStart,
    setVnMuted: (m) => {
      cfg.current.vnMuted = m;
      if (vn.current) vn.current.muted = m;
    },
    setVnVolume: (v) => {
      cfg.current.vnVol = v;
      const a = vn.current;
      if (a) {
        a.volume = Math.max(0, Math.min(1, v));
        a.muted = v <= 0;
      }
    },
    setOrigVolume: (v) => {
      cfg.current.origVol = v;
      if (video.current) video.current.volume = Math.max(0, Math.min(1, v));
    },
    setVideoOffset: (s) => {
      cfg.current.videoOffset = Math.max(0, s);
    },
    setVideoDur: (s) => {
      cfg.current.videoDur = Math.max(0, s);
    },
    setDuration: (s) => {
      durRef.current = Math.max(0, s);
      setDuration(Math.max(0, s));
    },
    onTime: noop,
    onPlay: noop,
    onPause: noop,
    onLoaded,
  };
}
