// Cloned voices are persisted ON DISK by studio-server (`/api/clones`, WAV files
// + SQLite). The vieneu TTS server keeps its encoded clone in memory (lost on
// restart), so for synthesis we lazily fetch the stored sample from studio and
// re-`/v1/clone` it to obtain a fresh `ref_id` per session.

import { cloneVoice } from "./api";
import {
  cloneSampleUrl,
  createClone,
  deleteClone as apiDeleteClone,
  listClones,
  renameClone as apiRenameClone,
  type VoiceClone,
} from "./studioApi";

export type ClonedVoice = VoiceClone; // { id, name, created_at }

// Session cache: clone id → vieneu ref_id (valid until vieneu restarts / reload).
const refCache = new Map<string, string>();

export function loadClones(): Promise<ClonedVoice[]> {
  return listClones();
}

/** Persist a recorded/uploaded WAV sample as a named clone (on disk via studio),
 *  and pre-register it with vieneu so the ref_id is ready this session. */
export async function addClone(name: string, wav: Blob): Promise<ClonedVoice> {
  const cv = await createClone(name.trim() || "Giọng của bạn", wav);
  try {
    const { ref_id } = await cloneVoice(wav, "ref.wav");
    refCache.set(cv.id, ref_id);
  } catch {
    /* ensureRefId will retry from the stored sample */
  }
  return cv;
}

export async function renameClone(id: string, name: string): Promise<ClonedVoice> {
  return apiRenameClone(id, name.trim() || "Giọng của bạn");
}

export async function removeClone(id: string): Promise<void> {
  refCache.delete(id);
  await apiDeleteClone(id);
}

/** A valid vieneu `ref_id` for a clone — re-uploads the stored sample (fetched
 *  from studio) if we don't have one cached this session. */
export async function ensureRefId(cv: ClonedVoice): Promise<string> {
  const cached = refCache.get(cv.id);
  if (cached) return cached;
  const sample = await fetch(await cloneSampleUrl(cv.id));
  if (!sample.ok) throw new Error(`tải mẫu giọng: ${sample.status}`);
  const { ref_id } = await cloneVoice(await sample.blob(), "ref.wav");
  refCache.set(cv.id, ref_id);
  return ref_id;
}
