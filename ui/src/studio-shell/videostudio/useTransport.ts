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
  // <video> event handlers
  onTime: () => void;
  onPlay: () => void;
  onPause: () => void;
  onLoaded: () => void;
}

/** Drives the real preview: a <video> (original) kept in sync with a hidden
 *  <audio> (Vietnamese track). Shared by the preview stage + timeline ruler. */
export function useTransport(): Transport {
  const video = useRef<HTMLVideoElement | null>(null);
  const vn = useRef<HTMLAudioElement | null>(null);
  const [time, setTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);

  const onTime = useCallback(() => {
    const v = video.current;
    const a = vn.current;
    if (!v) return;
    setTime(v.currentTime);
    if (a && Math.abs(a.currentTime - v.currentTime) > 0.3) a.currentTime = v.currentTime;
  }, []);

  const onPlay = useCallback(() => {
    setPlaying(true);
    vn.current?.play().catch(() => {});
  }, []);
  const onPause = useCallback(() => {
    setPlaying(false);
    vn.current?.pause();
  }, []);
  const onLoaded = useCallback(() => {
    if (video.current && Number.isFinite(video.current.duration)) setDuration(video.current.duration);
  }, []);

  const seek = useCallback((t: number) => {
    const v = video.current;
    if (!v) return;
    v.currentTime = Math.max(0, t);
    if (vn.current) vn.current.currentTime = Math.max(0, t);
    setTime(Math.max(0, t));
  }, []);

  const togglePlay = useCallback(() => {
    const v = video.current;
    if (!v) return;
    if (v.paused) void v.play();
    else v.pause();
  }, []);

  const toStart = useCallback(() => seek(0), [seek]);

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
      if (vn.current) vn.current.muted = m;
    },
    setVnVolume: (v) => {
      const a = vn.current;
      if (!a) return;
      a.volume = Math.max(0, Math.min(1, v));
      a.muted = v <= 0;
    },
    setOrigVolume: (v) => {
      if (video.current) video.current.volume = v;
    },
    onTime,
    onPlay,
    onPause,
    onLoaded,
  };
}
