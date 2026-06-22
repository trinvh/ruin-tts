// Undo/redo + history for the Video Studio track state. The "editor state" is
// the subset of the dub project that the timeline tracks control — toggling a
// track on/off (or the source/VN subtitle) records a history entry. Undo/redo
// re-applies a snapshot (persisted via patchSettings, so preview AND export stay
// in sync). History is kept in-session.

import { useCallback, useEffect, useRef, useState } from "react";
import type { DubProject } from "../../studioApi";
import type { DubProjectHook } from "./useDubProject";

export type EditorState = {
  video_enabled: boolean;
  original_volume: number;
  vn_volume: number;
  sub_bilingual: boolean; // source subtitle track (SZH)
  burn_subtitles: boolean; // Vietnamese subtitle track (SVI)
};

export type HistoryEntry = { label: string; state: EditorState };

function stateOf(p: DubProject): EditorState {
  return {
    video_enabled: p.video_enabled,
    original_volume: p.original_volume,
    vn_volume: p.vn_volume,
    sub_bilingual: p.sub_bilingual,
    burn_subtitles: p.burn_subtitles,
  };
}

export interface EditorHistory {
  entries: HistoryEntry[];
  cursor: number;
  canUndo: boolean;
  canRedo: boolean;
  /** Apply a track change + record it (folds in any slider edits since the last entry). */
  commit: (label: string, mutate: Partial<EditorState>) => void;
  undo: () => void;
  redo: () => void;
  jumpTo: (i: number) => void;
}

export function useEditorHistory(dub: DubProjectHook): EditorHistory {
  const proj = dub.detail?.project ?? null;
  const [hist, setHist] = useState<{ entries: HistoryEntry[]; cursor: number }>({
    entries: [],
    cursor: -1,
  });
  const histRef = useRef(hist);
  histRef.current = hist;
  const seeded = useRef<string | null>(null);

  // Seed the baseline entry once per opened project.
  useEffect(() => {
    if (!proj || seeded.current === proj.id) return;
    seeded.current = proj.id;
    setHist({ entries: [{ label: "Trạng thái ban đầu", state: stateOf(proj) }], cursor: 0 });
  }, [proj]);

  const apply = useCallback(
    (s: EditorState) => {
      void dub.patchSettings(s);
    },
    [dub],
  );

  const commit = useCallback(
    (label: string, mutate: Partial<EditorState>) => {
      if (!proj) return;
      const next = { ...stateOf(proj), ...mutate };
      apply(next);
      setHist((h) => {
        const head = h.cursor >= 0 ? h.entries.slice(0, h.cursor + 1) : [];
        const entries = [...head, { label, state: next }];
        return { entries, cursor: entries.length - 1 };
      });
    },
    [proj, apply],
  );

  const jumpTo = useCallback(
    (i: number) => {
      const h = histRef.current;
      if (i < 0 || i >= h.entries.length || i === h.cursor) return;
      apply(h.entries[i].state);
      setHist((cur) => ({ ...cur, cursor: i }));
    },
    [apply],
  );

  const undo = useCallback(() => jumpTo(histRef.current.cursor - 1), [jumpTo]);
  const redo = useCallback(() => jumpTo(histRef.current.cursor + 1), [jumpTo]);

  return {
    entries: hist.entries,
    cursor: hist.cursor,
    canUndo: hist.cursor > 0,
    canRedo: hist.cursor >= 0 && hist.cursor < hist.entries.length - 1,
    commit,
    undo,
    redo,
    jumpTo,
  };
}
