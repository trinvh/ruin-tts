import { useCallback, useEffect, useRef, useState } from "react";
import { getVoices, type Voice } from "../api";
import {
  cancelDub,
  createDubProject,
  deleteDubProject,
  getDubProject,
  listDubProjects,
  runDubStep,
  setDubSpeakerVoice,
  updateDubSegment,
  type DubDetail,
  type DubProject,
  type DubSegment,
  type DubStep,
} from "../studioApi";
import { isTauri, pickVideoFile, revealInDir } from "../platform";
import { Dropdown } from "../components/Dropdown";
import { VideoInfoBar } from "../components/dubbing/VideoInfoBar";
import { SettingsDialog } from "../components/dubbing/SettingsDialog";
import { Preview } from "../components/dubbing/Preview";
import { clock, genderLabel, speakerName, type VoiceOpt } from "../components/dubbing/shared";

const BUSY = (s: string) => s.endsWith("ing") && s !== "pending";

const STEPS: { step: DubStep; label: string; from: string; busy: string; done: string }[] = [
  { step: "extract", label: "Tách tiếng", from: "created", busy: "extracting", done: "extracted" },
  { step: "analyze", label: "Phân tích", from: "extracted", busy: "analyzing", done: "analyzed" },
  { step: "translate", label: "Dịch", from: "analyzed", busy: "translating", done: "translated" },
  { step: "synthesize", label: "Đọc TTS", from: "translated", busy: "synthesizing", done: "synthesized" },
  { step: "build", label: "Ghép track", from: "synthesized", busy: "building", done: "built" },
  { step: "export", label: "Xuất video", from: "built", busy: "exporting", done: "done" },
];
const ORDER = ["created", "extracted", "analyzed", "translated", "synthesized", "built", "done"];
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function DubbingPage() {
  const [projects, setProjects] = useState<DubProject[]>([]);
  const [selected, setSelected] = useState<string | null>(null);

  const refreshList = useCallback(async () => {
    try {
      setProjects(await listDubProjects());
    } catch {
      /* server may be starting */
    }
  }, []);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  const newProject = useCallback(async () => {
    const path = await pickVideoFile();
    if (!path) return;
    const name = path.split(/[/\\]/).pop()?.replace(/\.[^.]+$/, "") ?? "Dự án";
    const p = await createDubProject(name, path);
    await refreshList();
    setSelected(p.id);
  }, [refreshList]);

  if (selected) {
    return (
      <DubDetailView
        id={selected}
        onBack={() => { setSelected(null); void refreshList(); }}
        onDeleted={() => { setSelected(null); void refreshList(); }}
      />
    );
  }

  return (
    <div className="mx-auto w-full max-w-[1000px]">
      <div className="mb-5 flex items-end justify-between">
        <div>
          <h2 className="text-2xl font-semibold text-ink">Lồng tiếng video</h2>
          <p className="mt-1 text-sm text-muted">
            Nhập video tiếng Trung/Anh → tự tách giọng, dịch &amp; lồng tiếng Việt theo timestamp.
          </p>
        </div>
        <button className="rounded-md bg-brand px-4 py-2 text-sm font-medium text-white hover:opacity-90" onClick={newProject}>
          ＋ Dự án mới
        </button>
      </div>

      {projects.length === 0 ? (
        <div className="rounded-lg border border-dashed border-border p-10 text-center text-sm text-muted">
          Chưa có dự án. Bấm “Dự án mới” và chọn một file video để bắt đầu.
        </div>
      ) : (
        <div className="grid gap-2">
          {projects.map((p) => (
            <button
              key={p.id}
              onClick={() => setSelected(p.id)}
              className="flex items-center justify-between gap-3 rounded-lg border border-border bg-surface-2 px-4 py-3 text-left transition hover:border-brand"
            >
              <div className="min-w-0">
                <div className="font-medium text-ink">{p.name}</div>
                <div className="truncate text-xs text-muted">{p.video_path}</div>
              </div>
              <StatusBadge status={p.status} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const label: Record<string, string> = {
    created: "Mới tạo", extracting: "Đang tách tiếng…", extracted: "Đã tách tiếng",
    analyzing: "Đang phân tích…", analyzed: "Đã phân tích", translating: "Đang dịch…",
    translated: "Đã dịch", synthesizing: "Đang đọc…", synthesized: "Đã đọc",
    building: "Đang ghép…", built: "Đã ghép track", exporting: "Đang xuất…",
    done: "Hoàn tất", failed: "Lỗi", cancelled: "Đã huỷ",
  };
  const tone = status === "failed" ? "text-red-400 bg-red-500/10"
    : status === "done" ? "text-emerald-400 bg-emerald-500/10"
    : BUSY(status) ? "text-amber-400 bg-amber-500/10"
    : "text-muted bg-surface-2";
  return <span className={`shrink-0 rounded-full px-2.5 py-1 text-xs ${tone}`}>{label[status] ?? status}</span>;
}

function DubDetailView({ id, onBack, onDeleted }: { id: string; onBack: () => void; onDeleted: () => void }) {
  const [data, setData] = useState<DubDetail | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [autoRun, setAutoRun] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tab, setTab] = useState<"thoai" | "preview" | null>(null);

  const refresh = useCallback(async () => {
    try {
      setData(await getDubProject(id));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, [id]);

  useEffect(() => {
    void refresh();
    getVoices().then(setVoices).catch(() => {});
  }, [refresh]);

  const busy = data ? BUSY(data.project.status) : false;
  useEffect(() => {
    if (!busy) return;
    const h = setInterval(refresh, 1500);
    return () => clearInterval(h);
  }, [busy, refresh]);

  // Bump the media version only when the VN track was (re)built — i.e. when the
  // status leaves a synth/build/export phase. Settings saves don't touch status,
  // so the preview media URLs stay stable (no reload-to-black while editing).
  const [mediaVer, setMediaVer] = useState(0);
  const prevStatus = useRef("");
  useEffect(() => {
    const s = data?.project.status;
    if (!s) return;
    const prev = prevStatus.current;
    if ((prev === "synthesizing" || prev === "building" || prev === "exporting") && s !== prev) {
      setMediaVer((v) => v + 1);
    }
    prevStatus.current = s;
  }, [data?.project.status]);

  const statusNow = data?.project.status ?? "";
  const isNextStep = (step: DubStep) => STEPS.find((s) => s.step === step)?.from === statusNow;

  const act = useCallback(
    async (step: DubStep, force = false) => {
      if (!force && !isNextStep(step) && !BUSY(statusNow)) {
        const label = STEPS.find((s) => s.step === step)?.label ?? step;
        if (!confirm(`Chạy lại bước "${label}"? Bước này sẽ chạy lại (các bước sau cần chạy lại theo).`)) return;
      }
      setErr(null);
      try {
        await runDubStep(id, step);
        await refresh();
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [id, refresh, statusNow],
  );

  const runAll = useCallback(async () => {
    setErr(null);
    setAutoRun(true);
    try {
      let cur = await getDubProject(id);
      setData(cur);
      for (const s of STEPS) {
        if (ORDER.indexOf(s.done) <= ORDER.indexOf(cur.project.status)) continue;
        await runDubStep(id, s.step);
        for (;;) {
          await sleep(1500);
          cur = await getDubProject(id);
          setData(cur);
          if (cur.project.status === s.done) break;
          if (cur.project.status === "failed") throw new Error(cur.project.error ?? "lỗi không rõ");
        }
      }
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setAutoRun(false);
      await refresh();
    }
  }, [id, refresh]);

  if (!data) {
    return (
      <div className="mx-auto max-w-[1200px]">
        <button className="text-sm text-muted hover:text-ink" onClick={onBack}>← Danh sách</button>
        <div className="mt-6 text-sm text-muted">{err ?? "Đang tải…"}</div>
      </div>
    );
  }

  const { project, segments, speakers } = data;
  const status = project.status;
  const longCount = segments.filter((s) => s.status === "long").length;
  const reachedIdx = ORDER.indexOf(status === "failed" || BUSY(status) ? "" : status);
  const voiceOpts: VoiceOpt[] = voices.map((v) => ({ value: v.id, label: v.label }));
  const genderBySpeaker: Record<string, string | null> = Object.fromEntries(speakers.map((sp) => [sp.speaker, sp.gender]));

  const hasSegments = segments.length > 0;
  const canPreview = !!project.vn_track_path;
  const effTab: "thoai" | "preview" = tab ?? (canPreview ? "preview" : "thoai");

  return (
    <div className="mx-auto max-w-[1200px]">
      {/* Header */}
      <div className="mb-3 flex items-center gap-3">
        <button className="text-sm text-muted hover:text-ink" onClick={onBack}>← Danh sách</button>
        <h2 className="truncate text-xl font-semibold text-ink">{project.name}</h2>
        <StatusBadge status={status} />
        {project.language && <span className="rounded bg-surface-2 px-2 py-0.5 text-xs text-muted">nguồn: {project.language}</span>}
        {busy && (
          <button className="text-xs text-amber-300 underline" onClick={() => cancelDub(id).then(refresh)}>huỷ</button>
        )}
        <div className="ml-auto flex items-center gap-2">
          <button
            className="rounded-md border border-border px-2.5 py-1.5 text-sm text-muted hover:text-ink"
            onClick={() => setSettingsOpen(true)}
            title="Cấu hình dự án"
          >
            ⚙ Cấu hình
          </button>
          <button
            className="rounded-md px-2.5 py-1.5 text-sm text-red-400 hover:bg-red-500/10"
            onClick={async () => { if (confirm("Xoá dự án này?")) { await deleteDubProject(id); onDeleted(); } }}
          >
            Xoá
          </button>
        </div>
      </div>

      <div className="mb-4">
        <VideoInfoBar id={id} videoPath={project.video_path} />
      </div>

      {(err || project.error) && (
        <div className="mb-4 rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-300">
          {err ?? project.error}
        </div>
      )}

      {/* Stepper */}
      <Stepper
        status={status}
        reachedIdx={reachedIdx}
        busy={busy}
        autoRun={autoRun}
        onRun={act}
        onRunAll={runAll}
      />

      {/* Export banner */}
      {project.export_path && (
        <div className="mb-4 flex items-center gap-3 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-sm">
          <span className="truncate text-emerald-300">✓ Đã xuất: {project.export_path}</span>
          {isTauri() && (
            <button className="shrink-0 underline text-muted" onClick={() => revealInDir(project.export_path!)}>mở thư mục</button>
          )}
        </div>
      )}

      {/* Tabs */}
      <div className="mb-4 flex gap-1 border-b border-border">
        <Tab active={effTab === "thoai"} onClick={() => setTab("thoai")}>
          Câu thoại{hasSegments ? ` (${segments.length})` : ""}
        </Tab>
        <Tab active={effTab === "preview"} disabled={!canPreview} onClick={() => canPreview && setTab("preview")}>
          Nghe thử
        </Tab>
      </div>

      {effTab === "thoai" ? (
        hasSegments ? (
          <DialoguePanel
            id={id}
            segments={segments}
            speakers={speakers}
            voiceOpts={voiceOpts}
            genderBySpeaker={genderBySpeaker}
            longCount={longCount}
            busy={busy || autoRun}
            onChanged={refresh}
            onReshorten={() => act("reshorten", true)}
          />
        ) : (
          <Empty>Chạy bước <b>Phân tích</b> để tạo danh sách câu thoại.</Empty>
        )
      ) : canPreview ? (
        <Preview project={project} segments={segments} genderBySpeaker={genderBySpeaker} mediaVer={mediaVer} onSaved={refresh} />
      ) : (
        <Empty>Chạy đến bước <b>Ghép track</b> để nghe thử trên video.</Empty>
      )}

      <SettingsDialog project={project} open={settingsOpen} onClose={() => setSettingsOpen(false)} onSaved={refresh} />
    </div>
  );
}

function Stepper({
  status, reachedIdx, busy, autoRun, onRun, onRunAll,
}: {
  status: string;
  reachedIdx: number;
  busy: boolean;
  autoRun: boolean;
  onRun: (s: DubStep) => void;
  onRunAll: () => void;
}) {
  return (
    <div className="mb-4 flex flex-wrap items-center gap-1.5">
      {STEPS.map((s, i) => {
        const reached = ORDER.indexOf(s.done) <= reachedIdx;
        const isNext = s.from === status;
        const stepBusy = status === s.busy;
        return (
          <div key={s.step} className="flex items-center">
            {i > 0 && <span className={`mx-0.5 h-px w-4 ${reached ? "bg-brand/50" : "bg-border"}`} />}
            <button
              disabled={busy || autoRun}
              onClick={() => onRun(s.step)}
              title={reached && !isNext ? "Bấm để chạy lại (sẽ hỏi xác nhận)" : undefined}
              className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm transition ${
                stepBusy ? "bg-amber-500/20 text-amber-300"
                  : isNext ? "bg-brand text-white"
                  : reached ? "bg-emerald-500/10 text-emerald-300"
                  : "bg-surface-2 text-muted"
              } ${busy || autoRun ? "opacity-60" : "hover:opacity-90"}`}
            >
              <span className={`grid h-4 w-4 place-items-center rounded-full text-[10px] ${
                stepBusy ? "bg-amber-400/30" : isNext ? "bg-white/25" : reached ? "bg-emerald-400/25" : "bg-border"
              }`}>
                {stepBusy ? "…" : reached ? "✓" : i + 1}
              </span>
              {s.label}
            </button>
          </div>
        );
      })}
      <button
        disabled={busy || autoRun || status === "done"}
        onClick={onRunAll}
        className={`ml-auto rounded-full border border-brand px-3 py-1.5 text-sm text-brand transition ${
          busy || autoRun || status === "done" ? "opacity-50" : "hover:bg-brand hover:text-white"
        }`}
        title="Chạy tự động các bước còn lại cho đến khi xuất video"
      >
        {autoRun ? "⏳ Đang chạy…" : "▶▶ Chạy tất cả"}
      </button>
    </div>
  );
}

function Tab({ active, disabled, onClick, children }: { active: boolean; disabled?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      disabled={disabled}
      onClick={onClick}
      className={`-mb-px border-b-2 px-4 py-2 text-sm transition ${
        active ? "border-brand font-medium text-ink"
          : disabled ? "border-transparent text-muted/40"
          : "border-transparent text-muted hover:text-ink"
      }`}
    >
      {children}
    </button>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return <div className="rounded-lg border border-dashed border-border p-8 text-center text-sm text-muted">{children}</div>;
}

function DialoguePanel({
  id, segments, speakers, voiceOpts, genderBySpeaker, longCount, busy, onChanged, onReshorten,
}: {
  id: string;
  segments: DubSegment[];
  speakers: DubDetail["speakers"];
  voiceOpts: VoiceOpt[];
  genderBySpeaker: Record<string, string | null>;
  longCount: number;
  busy: boolean;
  onChanged: () => void;
  onReshorten: () => void;
}) {
  return (
    <div className="space-y-4">
      {/* Speaker → voice mapping */}
      {speakers.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {speakers.map((sp) => (
            <div key={sp.speaker} className="flex items-center gap-2 rounded-md border border-border bg-surface-2 px-2.5 py-1.5">
              <div className="text-xs">
                <span className="font-medium text-ink">{speakerName(sp.speaker)}</span>
                <span className="ml-1 text-muted">{genderLabel(sp.gender)}{sp.age ? ` · ~${Math.round(sp.age)}t` : ""}</span>
              </div>
              <div className="w-40">
                <Dropdown
                  value={sp.voice ?? ""}
                  options={[{ value: "", label: "(tự/mặc định)" }, ...voiceOpts]}
                  onChange={(v) => setDubSpeakerVoice(id, sp.speaker, v || null).then(onChanged)}
                  placeholder="Chọn giọng…"
                />
              </div>
            </div>
          ))}
        </div>
      )}

      {longCount > 0 && (
        <div className="flex items-center justify-between rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-300">
          <span>{longCount} câu quá dài so với thời lượng (phải tăng tốc kịch khung).</span>
          <button disabled={busy} onClick={onReshorten} className="rounded bg-amber-500/20 px-2.5 py-1 hover:bg-amber-500/30">
            Dịch ngắn lại
          </button>
        </div>
      )}

      <div className="overflow-hidden rounded-lg border border-border">
        {segments.map((s) => (
          <SegmentRow key={s.id} seg={s} gender={genderBySpeaker[s.speaker] ?? null} voiceOpts={voiceOpts} onChanged={onChanged} />
        ))}
      </div>
    </div>
  );
}

function SegmentRow({
  seg, gender, voiceOpts, onChanged,
}: {
  seg: DubSegment;
  gender: string | null;
  voiceOpts: VoiceOpt[];
  onChanged: () => void;
}) {
  const [vi, setVi] = useState(seg.text_vi);
  useEffect(() => setVi(seg.text_vi), [seg.text_vi]);
  const save = () => {
    if (vi !== seg.text_vi) void updateDubSegment(seg.id, vi, seg.voice).then(onChanged);
  };

  return (
    <div className="flex gap-3 border-b border-border px-3 py-2 last:border-0 hover:bg-surface-2/40">
      <div className="w-24 shrink-0 pt-1 text-xs text-muted">
        {clock(seg.start_s)}
        <div className="text-[11px] font-medium text-ink">{speakerName(seg.speaker)}</div>
        <div className="text-[10px]">{genderLabel(gender)}</div>
        {seg.status === "long" && <div className="text-[10px] text-amber-400">quá dài</div>}
        {seg.factor && seg.factor > 1.01 && <div className="text-[10px] text-muted">{seg.factor.toFixed(2)}×</div>}
      </div>
      <div className="flex-1">
        <div className="text-xs text-muted">{seg.text_src}</div>
        <textarea
          className="mt-1 w-full resize-none rounded border border-border bg-canvas px-2 py-1 text-sm text-ink focus:border-brand focus:outline-none"
          rows={1}
          value={vi}
          placeholder="(chưa dịch)"
          onChange={(e) => setVi(e.target.value)}
          onBlur={save}
        />
      </div>
      <div className="w-40 shrink-0 pt-1">
        <Dropdown
          value={seg.voice ?? ""}
          options={[{ value: "", label: "(theo người nói)" }, ...voiceOpts]}
          onChange={(v) => updateDubSegment(seg.id, seg.text_vi, v || null).then(onChanged)}
          placeholder="Giọng riêng…"
        />
      </div>
    </div>
  );
}
