// Generation queue: runs up to `concurrency` jobs in parallel against the
// server's worker pool, with pause/resume and per-item cancel. Each finished
// item is auto-saved to the output folder (Tauri) and is playable via its
// server download URL (no audio bytes kept in app memory).

import { useCallback, useEffect, useRef, useState } from "react";
import { cancelJob, createJob, getJob, jobDownloadUrl, type SynthParams } from "./api";
import { copyFile, isTauri, joinPath } from "./platform";

export type ItemStatus = "queued" | "running" | "done" | "failed" | "cancelled";

export type QueueItem = {
  id: string;
  label: string;
  voiceLabel: string;
  format: "wav" | "mp3";
  status: ItemStatus;
  jobId?: string;
  durationS?: number;
  url?: string; // server download URL (playback)
  serverPath?: string; // absolute path on this machine (for Save As / reveal)
  savedPath?: string; // where it was auto-saved
  error?: string;
};

export type Submit = { params: SynthParams; label: string; voiceLabel: string; fileBase: string };

let counter = 0;
const uid = () => `${Date.now().toString(36)}-${counter++}`;

function sanitize(name: string): string {
  return name.replace(/[^\p{L}\p{N} _-]/gu, "").trim().slice(0, 48).replace(/\s+/g, "_") || "audio";
}

/// Build a queue submission from synthesis params + the human voice label.
export function buildSubmit(params: SynthParams, voiceLabel: string): Submit {
  return {
    params,
    label: params.text.trim().slice(0, 60),
    voiceLabel,
    fileBase: `${sanitize(params.text)}_${Date.now().toString(36)}`,
  };
}

export function useQueue(outputDir: string, concurrency: number) {
  const [items, setItems] = useState<QueueItem[]>([]);
  const [paused, setPaused] = useState(false);

  const pausedRef = useRef(paused);
  const concRef = useRef(concurrency);
  const outRef = useRef(outputDir);
  const itemsRef = useRef(items);
  const pending = useRef<Array<{ id: string; sub: Submit }>>([]);
  const running = useRef(0);

  useEffect(() => { pausedRef.current = paused; }, [paused]);
  useEffect(() => { concRef.current = concurrency; }, [concurrency]);
  useEffect(() => { outRef.current = outputDir; }, [outputDir]);
  useEffect(() => { itemsRef.current = items; }, [items]);

  const patch = useCallback((id: string, p: Partial<QueueItem>) => {
    setItems((xs) => xs.map((x) => (x.id === id ? { ...x, ...p } : x)));
  }, []);

  const runItem = useRef<(id: string, sub: Submit) => Promise<void>>(async () => {});
  const pump = useCallback(() => {
    while (!pausedRef.current && running.current < concRef.current && pending.current.length > 0) {
      const { id, sub } = pending.current.shift()!;
      void runItem.current(id, sub);
    }
  }, []);

  runItem.current = async (id, sub) => {
    running.current += 1;
    patch(id, { status: "running" });
    try {
      const jobId = await createJob(sub.params);
      patch(id, { jobId });
      for (;;) {
        await new Promise((r) => setTimeout(r, 500));
        const view = await getJob(jobId);
        if (view.status === "cancelled") {
          patch(id, { status: "cancelled" });
          break;
        }
        if (view.status === "failed") {
          patch(id, { status: "failed", error: view.error ?? "failed" });
          break;
        }
        if (view.status === "done" && view.ready) {
          const url = await jobDownloadUrl(jobId);
          let savedPath: string | undefined;
          if (isTauri() && view.path && outRef.current) {
            const dest = joinPath(outRef.current, `${sub.fileBase}.${sub.params.format}`);
            if ((await copyFile(view.path, dest)) === null) savedPath = dest;
          }
          patch(id, {
            status: "done",
            url,
            serverPath: view.path ?? undefined,
            durationS: view.duration_s ?? undefined,
            savedPath,
          });
          break;
        }
      }
    } catch (e) {
      patch(id, { status: "failed", error: e instanceof Error ? e.message : String(e) });
    } finally {
      running.current -= 1;
      pump();
    }
  };

  const enqueue = useCallback((sub: Submit) => {
    const id = uid();
    setItems((xs) => [
      { id, label: sub.label, voiceLabel: sub.voiceLabel, format: sub.params.format, status: "queued" },
      ...xs,
    ]);
    pending.current.push({ id, sub });
    setTimeout(pump, 0);
  }, [pump]);

  const cancel = useCallback(async (id: string) => {
    const it = itemsRef.current.find((x) => x.id === id);
    if (!it) return;
    if (it.status === "queued") {
      pending.current = pending.current.filter((p) => p.id !== id);
      patch(id, { status: "cancelled" });
    } else if (it.status === "running" && it.jobId) {
      await cancelJob(it.jobId);
      patch(id, { status: "cancelled" });
    }
  }, [patch]);

  const cancelAll = useCallback(async () => {
    pending.current = [];
    for (const it of itemsRef.current) {
      if (it.status === "queued") patch(it.id, { status: "cancelled" });
      else if (it.status === "running" && it.jobId) {
        await cancelJob(it.jobId);
        patch(it.id, { status: "cancelled" });
      }
    }
  }, [patch]);

  const clearFinished = useCallback(() => {
    setItems((xs) => xs.filter((x) => x.status === "queued" || x.status === "running"));
  }, []);

  /// Remove a single item from the list (the "X" button) — cancels the job first
  /// if it's still queued/running, then drops it regardless of status.
  const remove = useCallback(async (id: string) => {
    const it = itemsRef.current.find((x) => x.id === id);
    if (it?.status === "queued") pending.current = pending.current.filter((p) => p.id !== id);
    if (it?.status === "running" && it.jobId) await cancelJob(it.jobId).catch(() => {});
    setItems((xs) => xs.filter((x) => x.id !== id));
  }, []);

  const stats = {
    queued: items.filter((x) => x.status === "queued").length,
    running: items.filter((x) => x.status === "running").length,
    done: items.filter((x) => x.status === "done").length,
  };

  return { items, paused, setPaused, enqueue, cancel, cancelAll, clearFinished, remove, stats };
}
