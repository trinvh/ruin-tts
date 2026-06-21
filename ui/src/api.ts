// HTTP client for vieneu-server — the API the demo showcases.

import { serverBase } from "./platform";

export type Voice = { id: string; label: string };
export type ServerInfo = { sample_rate: number; pool_size: number; voices: number };

export type SynthParams = {
  text: string;
  voice?: string;
  ref_id?: string;
  emotion: string;
  temperature: number;
  top_k: number;
  top_p: number;
  repetition_penalty: number;
  format: "wav" | "mp3";
};

export type JobStatus = "queued" | "running" | "done" | "failed" | "cancelled";
export type JobView = {
  status: JobStatus;
  duration_s: number | null;
  error: string | null;
  ready: boolean;
  path: string | null;
};

const FALLBACK_BASE = "http://127.0.0.1:8080";
let basePromise: Promise<string> | null = null;

export function base(): Promise<string> {
  if (!basePromise) basePromise = serverBase().then((b) => b ?? FALLBACK_BASE);
  return basePromise;
}

export async function getInfo(): Promise<ServerInfo> {
  const r = await fetch(`${await base()}/v1/info`);
  if (!r.ok) throw new Error(`info ${r.status}`);
  return r.json();
}

export async function getVoices(): Promise<Voice[]> {
  const r = await fetch(`${await base()}/v1/voices`);
  if (!r.ok) throw new Error(`voices ${r.status}`);
  return r.json();
}

export async function createJob(p: SynthParams): Promise<string> {
  const r = await fetch(`${await base()}/v1/jobs`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(p),
  });
  if (!r.ok) throw new Error(`job ${r.status}: ${await r.text().catch(() => "")}`);
  return (await r.json()).job_id as string;
}

export async function getJob(id: string): Promise<JobView> {
  const r = await fetch(`${await base()}/v1/jobs/${id}`);
  if (!r.ok) throw new Error(`job ${r.status}`);
  return r.json();
}

export async function cancelJob(id: string): Promise<void> {
  await fetch(`${await base()}/v1/jobs/${id}`, { method: "DELETE" }).catch(() => {});
}

export async function jobDownloadUrl(id: string): Promise<string> {
  return `${await base()}/v1/jobs/${id}/download`;
}

export async function cloneVoice(file: Blob, filename = "ref.wav"): Promise<{ ref_id: string; frames: number }> {
  const fd = new FormData();
  fd.append("file", file, filename);
  const r = await fetch(`${await base()}/v1/clone`, { method: "POST", body: fd });
  if (!r.ok) throw new Error(`clone ${r.status}: ${await r.text().catch(() => "")}`);
  return r.json();
}

/// Synthesize synchronously and return the audio blob (used for quick voice
/// previews — `/v1/tts` returns the encoded audio directly, no job needed).
export async function synthDirect(p: SynthParams): Promise<Blob> {
  const r = await fetch(`${await base()}/v1/tts`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(p),
  });
  if (!r.ok) throw new Error(`tts ${r.status}: ${await r.text().catch(() => "")}`);
  return r.blob();
}
