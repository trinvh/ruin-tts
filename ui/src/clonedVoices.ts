// Persisted voice clones. The vieneu server keeps clones in memory (lost on
// restart), so we persist the *reference sample* on the client and re-`/v1/clone`
// it to obtain a fresh `ref_id` whenever one is needed in a session.

import { cloneVoice } from "./api";

export type ClonedVoice = {
  id: string;
  name: string;
  createdAt: number;
  /** base64 of the recorded/uploaded reference clip (audio/webm or wav). */
  sampleB64: string;
  mime: string;
};

const KEY = "beesoft_cloned_voices";
// Session cache: cloned-voice id → server ref_id (valid until the server restarts
// or the page reloads). Re-obtained lazily via ensureRefId().
const refCache = new Map<string, string>();

export function loadClones(): ClonedVoice[] {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as ClonedVoice[]) : [];
  } catch {
    return [];
  }
}

function saveAll(list: ClonedVoice[]) {
  localStorage.setItem(KEY, JSON.stringify(list));
}

async function blobToB64(blob: Blob): Promise<string> {
  const buf = new Uint8Array(await blob.arrayBuffer());
  let bin = "";
  for (let i = 0; i < buf.length; i++) bin += String.fromCharCode(buf[i]);
  return btoa(bin);
}

function b64ToBlob(b64: string, mime: string): Blob {
  const bin = atob(b64);
  const arr = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
  return new Blob([arr], { type: mime });
}

/** Persist a recorded/uploaded sample as a named clone, and clone it now so the
 *  ref_id is ready this session. Returns the new list. */
export async function addClone(name: string, blob: Blob): Promise<ClonedVoice[]> {
  const id = "clone_" + Date.now().toString(36) + Math.floor(Math.random() * 1e4).toString(36);
  const cv: ClonedVoice = {
    id,
    name: name.trim() || "Giọng của bạn",
    createdAt: Date.now(),
    sampleB64: await blobToB64(blob),
    mime: blob.type || "audio/webm",
  };
  const list = [...loadClones(), cv];
  saveAll(list);
  // Best-effort: register with the server right away (ignore failure — ensureRefId
  // retries later).
  try {
    const ext = cv.mime.includes("wav") ? "wav" : "webm";
    const { ref_id } = await cloneVoice(blob, `ref.${ext}`);
    refCache.set(id, ref_id);
  } catch {
    /* will retry in ensureRefId */
  }
  return list;
}

export function removeClone(id: string): ClonedVoice[] {
  refCache.delete(id);
  const list = loadClones().filter((c) => c.id !== id);
  saveAll(list);
  return list;
}

/** Ensure a valid server `ref_id` for a clone, re-`/v1/clone`-ing the stored
 *  sample if we don't have one cached this session. Throws if the server is
 *  unreachable. */
export async function ensureRefId(cv: ClonedVoice): Promise<string> {
  const cached = refCache.get(cv.id);
  if (cached) return cached;
  const blob = b64ToBlob(cv.sampleB64, cv.mime);
  const ext = cv.mime.includes("wav") ? "wav" : "webm";
  const { ref_id } = await cloneVoice(blob, `ref.${ext}`);
  refCache.set(cv.id, ref_id);
  return ref_id;
}
