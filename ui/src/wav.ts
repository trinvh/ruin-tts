// Decode any browser-supported audio blob (mic recording in webm/mp4, or an
// uploaded mp3/m4a/…) and re-encode it as mono 16-bit PCM WAV, so the server's
// symphonia decoder reliably reads the voice-clone reference regardless of what
// format the platform's MediaRecorder produced.

export async function toWav(blob: Blob): Promise<Blob> {
  const AC: typeof AudioContext =
    window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
  const ctx = new AC();
  try {
    const audio = await ctx.decodeAudioData(await blob.arrayBuffer());
    return new Blob([encodeWav(audio)], { type: "audio/wav" });
  } finally {
    void ctx.close();
  }
}

function encodeWav(buf: AudioBuffer): ArrayBuffer {
  const len = buf.length;
  const sr = buf.sampleRate;
  // Downmix to mono.
  const data = new Float32Array(len);
  for (let c = 0; c < buf.numberOfChannels; c++) {
    const ch = buf.getChannelData(c);
    for (let i = 0; i < len; i++) data[i] += ch[i] / buf.numberOfChannels;
  }
  const out = new ArrayBuffer(44 + len * 2);
  const view = new DataView(out);
  const wstr = (off: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(off + i, s.charCodeAt(i));
  };
  wstr(0, "RIFF");
  view.setUint32(4, 36 + len * 2, true);
  wstr(8, "WAVE");
  wstr(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, sr, true);
  view.setUint32(28, sr * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  wstr(36, "data");
  view.setUint32(40, len * 2, true);
  let off = 44;
  for (let i = 0; i < len; i++) {
    const s = Math.max(-1, Math.min(1, data[i]));
    view.setInt16(off, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    off += 2;
  }
  return out;
}
