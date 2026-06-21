// Client for studio-server (the automation backend, spawned by the Tauri shell).

import { studioBase } from "./platform";

const FALLBACK = "http://127.0.0.1:8090";
let basePromise: Promise<string> | null = null;
export function base(): Promise<string> {
  if (!basePromise) basePromise = studioBase().then((b) => b ?? FALLBACK);
  return basePromise;
}

export type Novel = { id: string; slug: string; title: string; author: string | null; chapterCount: number; coverUrl: string | null };

export type NodeField = {
  key: string;
  label: string;
  kind: "novel" | "number" | "text" | "textarea" | "bool" | "select";
  options?: string[];
  default?: unknown;
};
export type NodeSpec = {
  type: string;
  label: string;
  desc?: string;
  fields: NodeField[];
  /** True for control-flow blocks (If/Loop) with multiple named output ports. */
  control?: boolean;
  /** Named source handles, e.g. ["then","else"] or ["body","done"]. */
  handles?: string[];
};

export type GraphNode = { id: string; type: string; config: Record<string, unknown>; position?: { x: number; y: number } };
export type GraphEdge = { from: string; to: string; handle?: string };
export type Graph = { id: string; name: string; version: number; nodes: GraphNode[]; edges: GraphEdge[] };

export type RunSummary = { id: string; status: string; preview: boolean; label: string; error: string | null; created_at: string; updated_at: string };
export type RunStep = {
  node_id: string;
  node_type: string;
  status: "pending" | "running" | "done" | "failed" | "cancelled";
  input: unknown;
  output: { logs?: string[]; state?: Record<string, unknown>; error?: string } | null;
  started_at: string | null;
  finished_at: string | null;
};
export type RunDetail = RunSummary & { steps: RunStep[] };

export type Profile = {
  site_name: string; voice: string; emotion: string; format: string;
  wpm: number; cap_seconds: number; overhead_seconds: number;
  width: number; height: number;
  background_path: string | null; background_is_video: boolean;
  intro_music_path: string | null; bg_music_path: string | null;
  duck: { music_volume: number; threshold: number; ratio: number; attack: number; release: number };
  intro_template: string; outro_template: string; title_template: string; description_template: string; tags_template: string;
  delay_before_intro: number; delay_after_intro: number; delay_after_content: number; delay_after_outro: number;
  voice_temperature: number; voice_top_k: number; voice_top_p: number; voice_repetition_penalty: number;
  segment_pause: number;
  paragraph_pause: number;
  workflow_version: number;
};
export type AppConfig = {
  ruin_base: string; ruin_key: string; tts_base: string;
  yt_client_id: string; yt_client_secret: string; yt_refresh_token: string; yt_privacy: string;
  media_ai_base: string; gemini_api_key: string; gemini_model: string;
  dub_voice_male: string; dub_voice_female: string;
  profile: Profile;
};

// ── Video dubbing ───────────────────────────────────────────────────────────
export type DubProject = {
  id: string; name: string; video_path: string; audio_path: string | null;
  status: string; error: string | null; language: string | null;
  gemini_model: string; original_volume: number; vn_volume: number; speed_cap: number;
  burn_subtitles: boolean; blur_subtitle: boolean;
  blur_x: number; blur_y: number; blur_w: number; blur_h: number; sub_y: number;
  sub_size: number; sub_color: string; sub_bilingual: boolean;
  vn_track_path: string | null; export_path: string | null;
  created_at: string; updated_at: string;
};
export type DubSettings = {
  name: string; gemini_model: string; original_volume: number; vn_volume: number; speed_cap: number;
  burn_subtitles: boolean; blur_subtitle: boolean;
  blur_x: number; blur_y: number; blur_w: number; blur_h: number; sub_y: number;
  sub_size: number; sub_color: string; sub_bilingual: boolean;
};
export type DubSegment = {
  id: string; project_id: string; idx: number; start_s: number; end_s: number;
  speaker: string; text_src: string; text_vi: string; voice: string | null;
  tts_path: string | null; fitted_path: string | null; factor: number | null; status: string;
};
export type DubSpeaker = { speaker: string; gender: string | null; age: number | null; voice: string | null };
export type DubDetail = { project: DubProject; segments: DubSegment[]; speakers: DubSpeaker[] };

async function j<T>(res: Response): Promise<T> {
  if (!res.ok) throw new Error(`${res.status}: ${await res.text().catch(() => "")}`);
  return res.json();
}

export async function searchNovels(search: string): Promise<{ items: Novel[] }> {
  const u = new URL(`${await base()}/api/novels`);
  if (search) u.searchParams.set("search", search);
  u.searchParams.set("limit", "20");
  return j(await fetch(u));
}
export async function getNodeSpecs(): Promise<NodeSpec[]> {
  return j(await fetch(`${await base()}/api/nodes`));
}
export async function getDefaultGraph(): Promise<Graph> {
  return j(await fetch(`${await base()}/api/workflow/default`));
}
export async function getLoopGraph(): Promise<Graph> {
  return j(await fetch(`${await base()}/api/workflow/loop`));
}
export async function listWorkflows(): Promise<Graph[]> {
  return j(await fetch(`${await base()}/api/workflows`));
}
export async function getWorkflow(id: string): Promise<Graph> {
  return j(await fetch(`${await base()}/api/workflows/${encodeURIComponent(id)}`));
}
export async function saveWorkflow(graph: Graph): Promise<void> {
  const r = await fetch(`${await base()}/api/workflows`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(graph) });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text().catch(() => "")}`);
}
export async function deleteWorkflow(id: string): Promise<void> {
  await fetch(`${await base()}/api/workflows/${encodeURIComponent(id)}`, { method: "DELETE" });
}
export async function retryRun(runId: string, fromNode: string): Promise<{ run_id: string }> {
  return j(await fetch(`${await base()}/api/runs/${runId}/retry?from=${encodeURIComponent(fromNode)}`, { method: "POST" }));
}
export async function createRun(graph: Graph, preview: boolean): Promise<{ run_id: string }> {
  return j(await fetch(`${await base()}/api/runs`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ graph, preview }) }));
}
export async function listRuns(): Promise<RunSummary[]> {
  return j(await fetch(`${await base()}/api/runs`));
}
export async function cancelRun(id: string): Promise<void> {
  const r = await fetch(`${await base()}/api/runs/${encodeURIComponent(id)}/cancel`, { method: "POST" });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text().catch(() => "")}`);
}
export async function clearRuns(): Promise<{ deleted: number }> {
  return j(await fetch(`${await base()}/api/runs`, { method: "DELETE" }));
}
export async function getRun(id: string): Promise<RunDetail> {
  return j(await fetch(`${await base()}/api/runs/${id}`));
}
export async function fileUrl(path: string): Promise<string> {
  return `${await base()}/api/file?path=${encodeURIComponent(path)}`;
}
export async function getConfig(): Promise<AppConfig> {
  return j(await fetch(`${await base()}/api/config`));
}
export async function putConfig(cfg: AppConfig): Promise<void> {
  const r = await fetch(`${await base()}/api/config`, { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify(cfg) });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text().catch(() => "")}`);
}

// ── Video dubbing API ───────────────────────────────────────────────────────
export async function listDubProjects(): Promise<DubProject[]> {
  const r = await j<{ projects: DubProject[] }>(await fetch(`${await base()}/api/dub/projects`));
  return r.projects;
}
export async function createDubProject(name: string, video_path: string): Promise<DubProject> {
  const r = await fetch(`${await base()}/api/dub/projects`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ name, video_path }),
  });
  return (await j<{ project: DubProject }>(r)).project;
}
export async function getDubProject(id: string): Promise<DubDetail> {
  return j(await fetch(`${await base()}/api/dub/projects/${id}`));
}
export async function deleteDubProject(id: string): Promise<void> {
  await fetch(`${await base()}/api/dub/projects/${id}`, { method: "DELETE" });
}
export async function updateDubSettings(id: string, s: DubSettings): Promise<void> {
  const r = await fetch(`${await base()}/api/dub/projects/${id}/settings`, {
    method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify(s),
  });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text().catch(() => "")}`);
}
export type DubStep = "extract" | "analyze" | "translate" | "synthesize" | "reshorten" | "build" | "export";
export async function runDubStep(id: string, step: DubStep): Promise<void> {
  const r = await fetch(`${await base()}/api/dub/projects/${id}/${step}`, { method: "POST" });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text().catch(() => "")}`);
}
export async function cancelDub(id: string): Promise<void> {
  await fetch(`${await base()}/api/dub/projects/${id}/cancel`, { method: "POST" });
}
export async function updateDubSegment(segId: string, text_vi: string, voice: string | null): Promise<void> {
  const r = await fetch(`${await base()}/api/dub/segments/${segId}`, {
    method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ text_vi, voice }),
  });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text().catch(() => "")}`);
}
export async function setDubSpeakerVoice(id: string, speaker: string, voice: string | null): Promise<void> {
  const r = await fetch(`${await base()}/api/dub/projects/${id}/speakers/${encodeURIComponent(speaker)}/voice`, {
    method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ voice }),
  });
  if (!r.ok) throw new Error(`${r.status}: ${await r.text().catch(() => "")}`);
}
export async function dubVideoUrl(id: string): Promise<string> {
  return `${await base()}/api/dub/projects/${id}/video`;
}
export type DubMediaInfo = {
  duration: number | null;
  size: number | null;
  format_name: string | null;
  video: { codec: string | null; profile: string | null; width: number | null; height: number | null; pix_fmt: string | null; fps: number | null; bit_rate: string | null } | null;
  audio: { codec: string | null; channels: number | null; sample_rate: string | null } | null;
};
export async function getDubInfo(id: string): Promise<DubMediaInfo> {
  return j(await fetch(`${await base()}/api/dub/projects/${id}/info`));
}
