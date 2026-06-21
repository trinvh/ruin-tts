import { useEffect, useRef, useState } from "react";
import { isTauri, pickDirectory } from "../platform";
import { useTtsSettings } from "../ttsSettings";
import { getConfig, putConfig, type AppConfig } from "../studioApi";
import { C, FONT, MONO, injectStudioStyles } from "../studio-shell/theme";

// ── Local-storage key for TTS format ────────────────────────────────────────
const TTS_FORMAT_KEY = "beesoft_tts_format";
const TTS_FORMAT_DEFAULT = "MP3 (192 kbps)";

// ── Nav structure ────────────────────────────────────────────────────────────
type SectionId =
  | "general"
  | "appearance"
  | "keys"
  | "dub"
  | "youtube"
  | "profile"
  | "security"
  | "about";

type NavItem = { id: SectionId; label: string; icon: string };
type NavGroup = { group: string; items: NavItem[] };

const NAV: NavGroup[] = [
  {
    group: "Chung",
    items: [
      { id: "general", label: "Đọc văn bản", icon: "🔊" },
      { id: "appearance", label: "Giao diện", icon: "🎨" },
    ],
  },
  {
    group: "Dịch vụ & tích hợp",
    items: [
      { id: "keys", label: "Khóa & Dịch vụ", icon: "🔑" },
      { id: "dub", label: "Lồng tiếng (Dịch)", icon: "🎬" },
      { id: "youtube", label: "YouTube", icon: "▶" },
      { id: "profile", label: "Hồ sơ dựng", icon: "📋" },
    ],
  },
  {
    group: "Ứng dụng",
    items: [
      { id: "security", label: "Bảo mật", icon: "🔒" },
      { id: "about", label: "Giới thiệu", icon: "ℹ" },
    ],
  },
];

const SECTION_META: Record<SectionId, { title: string; desc: string }> = {
  general: {
    title: "Đọc văn bản",
    desc: "Nơi lưu kết quả và số luồng xử lý đồng thời.",
  },
  appearance: {
    title: "Giao diện",
    desc: "Chủ đề, ngôn ngữ và hiệu ứng của ứng dụng.",
  },
  keys: {
    title: "Khóa & Dịch vụ",
    desc: "Khóa API và địa chỉ dịch vụ Ruin / Beesoft TTS.",
  },
  dub: {
    title: "Lồng tiếng (Dịch)",
    desc: "Cấu hình dịch thoại sang tiếng Việt và tách giọng theo người nói.",
  },
  youtube: {
    title: "YouTube",
    desc: "Kết nối tài khoản để đăng video trực tiếp từ Beesoft Studio.",
  },
  profile: {
    title: "Hồ sơ dựng",
    desc: "Thông tin mặc định áp dụng khi dựng & xuất video.",
  },
  security: {
    title: "Bảo mật",
    desc: "Khoá ứng dụng và quản lý dữ liệu nhạy cảm.",
  },
  about: { title: "Giới thiệu", desc: "Phiên bản và thông tin ứng dụng." },
};

// ── Shared style helpers ─────────────────────────────────────────────────────
const inputBase: React.CSSProperties = {
  background: C.inset,
  border: `1px solid ${C.borderInset}`,
  borderRadius: 8,
  color: C.ink,
  fontSize: 13,
  padding: "9px 12px",
  outline: "none",
  width: "100%",
  fontFamily: FONT,
};

const monoBase: React.CSSProperties = {
  ...inputBase,
  fontFamily: MONO,
  fontSize: 12.5,
};

const selectStyle: React.CSSProperties = {
  ...inputBase,
  maxWidth: 280,
  cursor: "pointer",
  appearance: "none" as const,
  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='8' viewBox='0 0 12 8'%3E%3Cpath d='M1 1l5 5 5-5' stroke='%238b8f9e' stroke-width='1.5' fill='none' stroke-linecap='round'/%3E%3C/svg%3E")`,
  backgroundRepeat: "no-repeat",
  backgroundPosition: "right 10px center",
  paddingRight: 30,
};

const textareaStyle: React.CSSProperties = {
  ...inputBase,
  height: 78,
  resize: "vertical" as const,
};

const todoTagStyle: React.CSSProperties = {
  display: "inline-block",
  fontSize: 10,
  fontWeight: 600,
  padding: "1px 6px",
  borderRadius: 4,
  background: "rgba(255,181,114,0.15)",
  color: C.orange,
  border: `1px solid rgba(255,181,114,0.3)`,
  marginLeft: 6,
  verticalAlign: "middle",
};

// ── Sub-components ────────────────────────────────────────────────────────────

interface FieldRowProps {
  label: string;
  hint?: string;
  disabled?: boolean;
  todoTag?: boolean;
  children: React.ReactNode;
}

function FieldRow({ label, hint, disabled, todoTag, children }: FieldRowProps) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        padding: "13px 0",
        borderBottom: `1px solid #232531`,
        gap: 16,
        opacity: disabled ? 0.55 : 1,
      }}
    >
      <div style={{ width: 200, flexShrink: 0 }}>
        <div
          style={{
            color: C.ink5,
            fontSize: 13.5,
            fontWeight: 500,
            lineHeight: 1.4,
          }}
        >
          {label}
          {todoTag && <span style={todoTagStyle}>TODO</span>}
        </div>
        {hint && (
          <div
            style={{
              color: C.muted3,
              fontSize: 11.5,
              marginTop: 3,
              lineHeight: 1.4,
            }}
          >
            {hint}
          </div>
        )}
      </div>
      <div style={{ flex: 1 }}>{children}</div>
    </div>
  );
}

interface FocusInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  mono?: boolean;
}

function FocusInput({ mono, style, ...rest }: FocusInputProps) {
  const base = mono ? monoBase : inputBase;
  const [focused, setFocused] = useState(false);
  return (
    <input
      {...rest}
      style={{
        ...base,
        ...style,
        border: focused
          ? `1px solid ${C.purple}`
          : `1px solid ${C.borderInset}`,
      }}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
    />
  );
}

function NoteBlock({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        color: C.muted,
        fontSize: 12.5,
        background: C.inset,
        border: `1px solid #2a2833`,
        borderRadius: 9,
        padding: "11px 13px",
        lineHeight: 1.55,
        marginBottom: 4,
      }}
    >
      {children}
    </div>
  );
}

interface ToggleProps {
  value: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}

function Toggle({ value, onChange, disabled }: ToggleProps) {
  return (
    <div
      onClick={() => !disabled && onChange(!value)}
      style={{
        width: 44,
        height: 26,
        borderRadius: 13,
        background: value ? C.purple : "#393C49",
        position: "relative",
        cursor: disabled ? "not-allowed" : "pointer",
        transition: "background 0.2s",
        flexShrink: 0,
      }}
    >
      <div
        style={{
          position: "absolute",
          top: 3,
          left: value ? 21 : 3,
          width: 20,
          height: 20,
          borderRadius: "50%",
          background: "#fff",
          transition: "left 0.2s",
          boxShadow: "0 1px 3px rgba(0,0,0,0.4)",
        }}
      />
    </div>
  );
}

// ── Section: General ─────────────────────────────────────────────────────────

interface SectionGeneralProps {
  tts: ReturnType<typeof useTtsSettings>;
}

function SectionGeneral({ tts }: SectionGeneralProps) {
  const [ttsFormat, setTtsFormatState] = useState(
    () => localStorage.getItem(TTS_FORMAT_KEY) ?? TTS_FORMAT_DEFAULT
  );

  const setTtsFormat = (v: string) => {
    setTtsFormatState(v);
    localStorage.setItem(TTS_FORMAT_KEY, v);
  };

  const browseDir = async () => {
    const d = await pickDirectory();
    if (d) tts.setOutputDir(d);
  };

  return (
    <>
      <FieldRow label="Thư mục lưu" hint="Nơi lưu các file audio đã tạo">
        <div style={{ display: "flex", gap: 8 }}>
          <FocusInput
            mono
            value={tts.outputDir || ""}
            onChange={(e) => tts.setOutputDir(e.target.value)}
            placeholder="/đường/dẫn/lưu"
            style={{ flex: 1 }}
            disabled={!isTauri()}
          />
          <button
            onClick={browseDir}
            disabled={!isTauri()}
            style={{
              border: `1px solid ${C.border}`,
              background: C.panel2,
              color: C.steel,
              borderRadius: 7,
              padding: "0 12px",
              fontSize: 12.5,
              cursor: isTauri() ? "pointer" : "not-allowed",
              whiteSpace: "nowrap",
              fontFamily: FONT,
              opacity: isTauri() ? 1 : 0.5,
            }}
          >
            Đổi…
          </button>
        </div>
        {!isTauri() && (
          <div style={{ color: C.muted3, fontSize: 11.5, marginTop: 4 }}>
            Chọn thư mục chỉ khả dụng trong ứng dụng desktop.
          </div>
        )}
      </FieldRow>

      <FieldRow label="Số luồng song song" hint="Số audio tạo cùng lúc (1–8)">
        <FocusInput
          type="number"
          min={1}
          max={8}
          value={tts.concurrency}
          onChange={(e) => tts.setConcurrency(Number(e.target.value))}
          style={{ maxWidth: 100 }}
        />
      </FieldRow>

      <FieldRow label="Định dạng xuất audio" hint="Định dạng file audio mặc định">
        <select
          value={ttsFormat}
          onChange={(e) => setTtsFormat(e.target.value)}
          style={selectStyle}
        >
          <option>MP3 (192 kbps)</option>
          <option>MP3 (320 kbps)</option>
          <option>WAV (48 kHz)</option>
          <option>FLAC</option>
        </select>
      </FieldRow>
    </>
  );
}

// ── Section: Appearance ───────────────────────────────────────────────────────

function SectionAppearance() {
  return (
    <>
      <FieldRow
        label="Chủ đề"
        hint="Chưa hỗ trợ — app hiện chỉ chế độ tối."
        disabled
        todoTag
      >
        <select
          value="Tối"
          disabled
          style={{ ...selectStyle, cursor: "not-allowed" }}
        >
          <option>Tối</option>
          <option>Sáng</option>
          <option>Theo hệ thống</option>
        </select>
      </FieldRow>

      <FieldRow
        label="Ngôn ngữ"
        hint="Chưa hỗ trợ — app hiện chỉ tiếng Việt."
        disabled
        todoTag
      >
        <select
          value="Tiếng Việt"
          disabled
          style={{ ...selectStyle, cursor: "not-allowed" }}
        >
          <option>Tiếng Việt</option>
          <option>English</option>
        </select>
      </FieldRow>

      <FieldRow
        label="Hiệu ứng chuyển động"
        hint="Chưa hỗ trợ."
        disabled
        todoTag
      >
        <Toggle value={true} onChange={() => {}} disabled />
      </FieldRow>
    </>
  );
}

// ── Section: Keys ─────────────────────────────────────────────────────────────

interface SectionKeysProps {
  cfg: AppConfig;
  set: (p: Partial<AppConfig>) => void;
}

function SectionKeys({ cfg, set }: SectionKeysProps) {
  return (
    <>
      <FieldRow label="Ruin API key" hint="Khóa truy cập Ruin API">
        <FocusInput
          mono
          type="password"
          value={cfg.ruin_key}
          onChange={(e) => set({ ruin_key: e.target.value })}
          placeholder="ruin_…"
        />
      </FieldRow>
      <FieldRow label="Ruin API base" hint="URL máy chủ Ruin">
        <FocusInput
          mono
          value={cfg.ruin_base}
          onChange={(e) => set({ ruin_base: e.target.value })}
          placeholder="https://api.ruin.vn"
        />
      </FieldRow>
      <FieldRow label="Beesoft TTS base" hint="URL máy chủ TTS">
        <FocusInput
          mono
          value={cfg.tts_base}
          onChange={(e) => set({ tts_base: e.target.value })}
          placeholder="http://127.0.0.1:8080"
        />
      </FieldRow>
    </>
  );
}

// ── Section: Dub ──────────────────────────────────────────────────────────────

interface SectionDubProps {
  cfg: AppConfig;
  set: (p: Partial<AppConfig>) => void;
}

function SectionDub({ cfg, set }: SectionDubProps) {
  return (
    <>
      <NoteBlock>
        Nhận diện giọng nói (ASR), tách người nói (diarization) và phân tích
        giới tính/độ tuổi chạy <strong>hoàn toàn trên máy</strong> — không cần
        kết nối internet — thông qua sidecar Rust bundled sẵn trong ứng dụng
        (tự khởi động cùng app). Dịch thoại sang tiếng Việt sử dụng{" "}
        <strong>Gemini</strong>; cần khoá API bên dưới.
      </NoteBlock>

      <FieldRow label="Gemini API key" hint="Dùng để dịch thoại sang tiếng Việt">
        <FocusInput
          mono
          type="password"
          value={cfg.gemini_api_key}
          onChange={(e) => set({ gemini_api_key: e.target.value })}
          placeholder="AIza…"
        />
      </FieldRow>
      <FieldRow label="Gemini model" hint="Ví dụ: gemini-2.5-flash">
        <FocusInput
          mono
          value={cfg.gemini_model}
          onChange={(e) => set({ gemini_model: e.target.value })}
          placeholder="gemini-2.5-flash"
        />
      </FieldRow>
      <FieldRow label="Media-AI base" hint="URL sidecar nhận diện giọng (tự động)">
        <FocusInput
          mono
          value={cfg.media_ai_base}
          onChange={(e) => set({ media_ai_base: e.target.value })}
          placeholder="http://127.0.0.1:8099"
        />
      </FieldRow>
      <FieldRow
        label="Giọng nam ưu tiên"
        hint="Để trống = tự chọn theo giới tính"
      >
        <FocusInput
          mono
          value={cfg.dub_voice_male}
          onChange={(e) => set({ dub_voice_male: e.target.value })}
          placeholder="để trống = tự chọn"
        />
      </FieldRow>
      <FieldRow
        label="Giọng nữ ưu tiên"
        hint="Để trống = tự chọn theo giới tính"
      >
        <FocusInput
          mono
          value={cfg.dub_voice_female}
          onChange={(e) => set({ dub_voice_female: e.target.value })}
          placeholder="để trống = tự chọn"
        />
      </FieldRow>

      <div style={{ marginTop: 8 }}>
        <NoteBlock>
          Khi phân tích, mỗi người nói tự được gán giọng theo giới tính — phân
          loại từ tên giọng Beesoft (chứa "nam"/"nữ"). Nhiều người cùng giới
          nhận giọng khác nhau. Hai ô trên chỉ để ưu tiên một giọng cụ thể; bạn
          luôn chỉnh tay lại trong trang Lồng tiếng.
        </NoteBlock>
      </div>
    </>
  );
}

// ── Section: YouTube ──────────────────────────────────────────────────────────

interface SectionYoutubeProps {
  cfg: AppConfig;
  set: (p: Partial<AppConfig>) => void;
}

function SectionYoutube({ cfg, set }: SectionYoutubeProps) {
  return (
    <>
      <FieldRow label="Client ID" hint="OAuth2 Client ID từ Google Console">
        <FocusInput
          mono
          value={cfg.yt_client_id}
          onChange={(e) => set({ yt_client_id: e.target.value })}
        />
      </FieldRow>
      <FieldRow label="Client secret" hint="OAuth2 Client Secret">
        <FocusInput
          mono
          type="password"
          value={cfg.yt_client_secret}
          onChange={(e) => set({ yt_client_secret: e.target.value })}
        />
      </FieldRow>
      <FieldRow label="Refresh token" hint="Token làm mới OAuth2">
        <FocusInput
          mono
          value={cfg.yt_refresh_token}
          onChange={(e) => set({ yt_refresh_token: e.target.value })}
        />
      </FieldRow>
      <FieldRow label="Quyền riêng tư mặc định" hint="Áp dụng khi đăng video mới">
        <select
          value={cfg.yt_privacy}
          onChange={(e) => set({ yt_privacy: e.target.value })}
          style={selectStyle}
        >
          <option value="private">Riêng tư (private)</option>
          <option value="unlisted">Không công khai (unlisted)</option>
          <option value="public">Công khai (public)</option>
        </select>
      </FieldRow>
    </>
  );
}

// ── Section: Profile ──────────────────────────────────────────────────────────

const VOICE_OPTIONS = [
  "nu-nhanh-1",
  "nu-nhanh-2",
  "nu-nhanh-3",
  "nam-nhanh-1",
  "nam-nhanh-2",
  "nu-truyen-1",
  "nu-truyen-2",
  "nam-truyen-1",
];

interface SectionProfileProps {
  cfg: AppConfig;
  set: (p: Partial<AppConfig>) => void;
}

function SectionProfile({ cfg, set }: SectionProfileProps) {
  const setP = (patch: Partial<AppConfig["profile"]>) =>
    set({ profile: { ...cfg.profile, ...patch } });

  const voiceOptions = VOICE_OPTIONS.includes(cfg.profile.voice)
    ? VOICE_OPTIONS
    : [cfg.profile.voice, ...VOICE_OPTIONS];

  return (
    <>
      <FieldRow label="Tên kênh" hint="Tên kênh YouTube / trang web">
        <FocusInput
          value={cfg.profile.site_name}
          onChange={(e) => setP({ site_name: e.target.value })}
        />
      </FieldRow>
      <FieldRow label="Giọng đọc mặc định" hint="Giọng dùng khi dựng audio">
        <select
          value={cfg.profile.voice}
          onChange={(e) => setP({ voice: e.target.value })}
          style={selectStyle}
        >
          {voiceOptions.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      </FieldRow>
      <FieldRow
        label="Hashtags kênh"
        hint="Chưa được lưu vào backend."
        disabled
        todoTag
      >
        <textarea
          disabled
          style={{ ...textareaStyle, cursor: "not-allowed" }}
          placeholder="#beesoft #truyện #audiobook"
        />
      </FieldRow>
      <FieldRow
        label="Mô tả kênh"
        hint="Chưa được lưu vào backend."
        disabled
        todoTag
      >
        <textarea
          disabled
          style={{ ...textareaStyle, cursor: "not-allowed" }}
          placeholder="Mô tả kênh YouTube…"
        />
      </FieldRow>
    </>
  );
}

// ── Section: Security ─────────────────────────────────────────────────────────

interface SectionSecurityProps {
  cfg: AppConfig;
  set: (p: Partial<AppConfig>) => void;
}

function SectionSecurity({ cfg, set }: SectionSecurityProps) {
  const handleWipeKeys = async () => {
    const ok = window.confirm(
      "Xoá toàn bộ khoá? Thao tác này sẽ xoá Ruin API key, Gemini API key, YouTube client secret và refresh token. Không thể hoàn tác."
    );
    if (!ok) return;
    const patch: Partial<AppConfig> = {
      ruin_key: "",
      gemini_api_key: "",
      yt_client_secret: "",
      yt_refresh_token: "",
    };
    set(patch);
    try {
      await putConfig({ ...cfg, ...patch });
    } catch {
      // ignore — dirty flag will let user retry via save button
    }
  };

  return (
    <>
      <FieldRow
        label="Khoá ứng dụng"
        hint="Chưa hỗ trợ."
        disabled
        todoTag
      >
        <Toggle value={false} onChange={() => {}} disabled />
      </FieldRow>
      <FieldRow
        label="Xoá file tạm khi thoát"
        hint="Chưa hỗ trợ."
        disabled
        todoTag
      >
        <Toggle value={false} onChange={() => {}} disabled />
      </FieldRow>
      <FieldRow label="Xoá toàn bộ khoá" hint="Xoá tất cả API key đã lưu ngay lập tức">
        <button
          onClick={handleWipeKeys}
          style={{
            border: `1px solid rgba(255,124,163,0.4)`,
            background: `rgba(255,124,163,0.1)`,
            color: C.pink,
            borderRadius: 8,
            padding: "7px 14px",
            fontSize: 13,
            cursor: "pointer",
            fontFamily: FONT,
            fontWeight: 500,
          }}
        >
          Xoá toàn bộ khoá
        </button>
      </FieldRow>
    </>
  );
}

// ── Section: About ────────────────────────────────────────────────────────────

function SectionAbout() {
  return (
    <div style={{ padding: "8px 0" }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 16,
          padding: "16px 0",
        }}
      >
        <div
          style={{
            width: 52,
            height: 52,
            borderRadius: 12,
            background: "linear-gradient(160deg, #9288E0, #6f64c4)",
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 22,
          }}
        >
          🎙
        </div>
        <div>
          <div
            style={{
              color: C.ink,
              fontSize: 17,
              fontWeight: 700,
              lineHeight: 1.3,
            }}
          >
            Beesoft Studio
          </div>
          <div
            style={{
              color: C.muted,
              fontSize: 12.5,
              fontFamily: MONO,
              marginTop: 3,
            }}
          >
            v0.1.0 · on-device
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

type SaveStatus = "saved" | "unsaved" | "saving" | "error";

export function SettingsPage() {
  const tts = useTtsSettings();
  const [active, setActive] = useState<SectionId>("general");
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [loading, setLoading] = useState(true);
  const [dirty, setDirty] = useState(false);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("saved");
  const [saveError, setSaveError] = useState("");
  // Keep ref so wipeKeys in security section can merge latest cfg
  const cfgRef = useRef<AppConfig | null>(null);
  useEffect(() => {
    cfgRef.current = cfg;
  }, [cfg]);

  const load = async () => {
    setLoadError(false);
    setLoading(true);
    for (let attempt = 0; attempt < 12; attempt++) {
      try {
        const c = await getConfig();
        setCfg(c);
        setLoading(false);
        setSaveStatus("saved");
        setDirty(false);
        return;
      } catch {
        await new Promise<void>((r) => setTimeout(r, 700));
      }
    }
    setLoadError(true);
    setLoading(false);
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const set = (patch: Partial<AppConfig>) => {
    setCfg((prev) => (prev ? { ...prev, ...patch } : prev));
    setDirty(true);
    setSaveStatus("unsaved");
    setSaveError("");
  };

  const handleSave = async () => {
    if (!cfg) return;
    setSaveStatus("saving");
    try {
      await putConfig(cfg);
      setSaveStatus("saved");
      setDirty(false);
    } catch (e: unknown) {
      setSaveStatus("error");
      setSaveError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    injectStudioStyles();
  }, []);

  // ── Loading / error state ─────────────────────────────────────────────────
  if (loading || loadError || !cfg) {
    return (
      <div
        className="bss"
        style={{
          height: "100%",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          fontFamily: FONT,
          background: C.content,
          color: C.ink,
        }}
      >
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexDirection: "column",
            gap: 12,
          }}
        >
          {loadError ? (
            <>
              <div style={{ color: C.muted, fontSize: 13.5 }}>
                Không kết nối được máy chủ tự động hóa (cổng 8090).
              </div>
              <button
                onClick={load}
                style={{
                  background: C.panel2,
                  border: `1px solid ${C.border}`,
                  color: C.steel,
                  borderRadius: 8,
                  padding: "7px 16px",
                  fontSize: 13,
                  cursor: "pointer",
                  fontFamily: FONT,
                }}
              >
                Thử lại
              </button>
            </>
          ) : (
            <div style={{ color: C.muted, fontSize: 13.5 }}>
              Đang tải cấu hình…
            </div>
          )}
        </div>
      </div>
    );
  }

  const meta = SECTION_META[active];
  const pillSaved = saveStatus === "saved" || saveStatus === "saving";

  // ── Main layout ───────────────────────────────────────────────────────────
  return (
    <div
      className="bss"
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        fontFamily: FONT,
        background: C.content,
        color: C.ink,
      }}
    >
      {/* ── Top bar ── */}
      <div
        style={{
          height: 48,
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "0 20px",
          background: C.panel,
          borderBottom: `1px solid ${C.border}`,
          flexShrink: 0,
        }}
      >
        {/* Gear icon tile */}
        <div
          style={{
            width: 32,
            height: 32,
            borderRadius: 8,
            background: "rgba(171,187,194,0.14)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 16,
            color: "#ABBBC2",
            flexShrink: 0,
          }}
        >
          ⚙
        </div>

        {/* Title + subtitle */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{ fontSize: 14.5, fontWeight: 600, lineHeight: 1.2, color: C.ink }}
          >
            Cài đặt
          </div>
          <div style={{ fontSize: 11, color: C.muted, lineHeight: 1.2 }}>
            Tùy chọn đọc văn bản & cấu hình tự động hóa
          </div>
        </div>

        {/* Saved pill */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: 12,
            color: pillSaved ? C.teal : C.orange,
            padding: "4px 10px",
            borderRadius: 12,
            background: pillSaved
              ? "rgba(80,209,170,0.1)"
              : "rgba(255,181,114,0.1)",
            border: `1px solid ${
              pillSaved
                ? "rgba(80,209,170,0.25)"
                : "rgba(255,181,114,0.25)"
            }`,
          }}
        >
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: pillSaved ? C.teal : C.orange,
              display: "inline-block",
            }}
          />
          {saveStatus === "saving" ? "Đang lưu…" : pillSaved ? "Đã lưu" : "Chưa lưu"}
        </div>

        {/* Save button */}
        <button
          onClick={handleSave}
          disabled={!dirty || saveStatus === "saving"}
          style={{
            height: 34,
            background: dirty && saveStatus !== "saving" ? C.purple : C.panel2,
            border: "none",
            borderRadius: 8,
            color: dirty && saveStatus !== "saving" ? "#fff" : C.muted,
            fontSize: 13,
            fontWeight: 600,
            padding: "0 14px",
            cursor: dirty && saveStatus !== "saving" ? "pointer" : "not-allowed",
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontFamily: FONT,
            transition: "background 0.15s",
          }}
        >
          <span style={{ fontSize: 14 }}>💾</span>
          Lưu thay đổi
        </button>
      </div>

      {/* Save error banner */}
      {saveStatus === "error" && saveError && (
        <div
          style={{
            background: "rgba(255,124,163,0.1)",
            borderBottom: `1px solid rgba(255,124,163,0.3)`,
            color: C.pink,
            fontSize: 12.5,
            padding: "6px 20px",
            flexShrink: 0,
          }}
        >
          Lỗi khi lưu: {saveError}
        </div>
      )}

      {/* ── Body: sidebar + content ── */}
      <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
        {/* Sidebar */}
        <div
          style={{
            width: 248,
            flexShrink: 0,
            background: C.panel,
            borderRight: `1px solid ${C.border}`,
            overflowY: "auto",
            padding: "12px 0",
          }}
        >
          {NAV.map((group) => (
            <div key={group.group} style={{ marginBottom: 4 }}>
              <div
                style={{
                  color: C.muted3,
                  fontSize: 10.5,
                  fontWeight: 600,
                  letterSpacing: "0.07em",
                  textTransform: "uppercase",
                  padding: "8px 16px 4px",
                }}
              >
                {group.group}
              </div>
              {group.items.map((item) => {
                const isActive = active === item.id;
                return (
                  <SidebarItem
                    key={item.id}
                    item={item}
                    isActive={isActive}
                    onClick={() => setActive(item.id)}
                  />
                );
              })}
            </div>
          ))}
        </div>

        {/* Content area */}
        <div
          style={{
            flex: 1,
            overflowY: "auto",
            padding: "28px 32px",
            background: C.content,
          }}
        >
          <div style={{ maxWidth: 780 }}>
            <h1
              style={{
                fontSize: 22,
                fontWeight: 700,
                margin: "0 0 6px",
                color: C.ink,
              }}
            >
              {meta.title}
            </h1>
            <p
              style={{
                fontSize: 13.5,
                color: C.muted,
                margin: "0 0 20px",
                lineHeight: 1.5,
              }}
            >
              {meta.desc}
            </p>

            {active === "general" && <SectionGeneral tts={tts} />}
            {active === "appearance" && <SectionAppearance />}
            {active === "keys" && <SectionKeys cfg={cfg} set={set} />}
            {active === "dub" && <SectionDub cfg={cfg} set={set} />}
            {active === "youtube" && <SectionYoutube cfg={cfg} set={set} />}
            {active === "profile" && <SectionProfile cfg={cfg} set={set} />}
            {active === "security" && <SectionSecurity cfg={cfg} set={set} />}
            {active === "about" && <SectionAbout />}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Sidebar item (extracted to avoid inline arrow in JSX + allows hover) ──────

interface SidebarItemProps {
  item: NavItem;
  isActive: boolean;
  onClick: () => void;
}

function SidebarItem({ item, isActive, onClick }: SidebarItemProps) {
  const [hovered, setHovered] = useState(false);

  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 9,
        padding: "7px 16px",
        cursor: "pointer",
        background: isActive
          ? "rgba(146,136,224,0.12)"
          : hovered
          ? "#252836"
          : "transparent",
        borderLeft: isActive
          ? `3px solid ${C.purple}`
          : "3px solid transparent",
        color: isActive ? C.ink : C.steel,
        fontWeight: isActive ? 600 : 500,
        fontSize: 13.5,
        transition: "background 0.12s",
      }}
    >
      <span
        style={{
          fontSize: 15,
          color: isActive ? C.purpleLt : C.muted4,
        }}
      >
        {item.icon}
      </span>
      {item.label}
    </div>
  );
}
