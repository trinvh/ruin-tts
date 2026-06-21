import { C, MONO } from "./theme";
import { Icon, type IconName } from "./icons";
import { HoverBox } from "./ui";
import type { FeatureKey } from "./tabs";

export interface RecentItem {
  id: string;
  title: string;
  dur: string;
  statusLabel: string;
}

interface Props {
  onOpenFeature: (key: FeatureKey) => void;
  recents: RecentItem[];
  onOpenProject: (id: string) => void;
}

const FEATURES: { key: FeatureKey; title: string; desc: string; icon: IconName; color: string; tile: string }[] = [
  { key: "dub", title: "Lồng tiếng", desc: "Dịch & lồng tiếng video tiếng Trung / Anh sang tiếng Việt theo timestamp.", icon: "film", color: C.purpleLt, tile: "rgba(146,136,224,.18)" },
  { key: "tts", title: "Đọc (TTS)", desc: "Chuyển văn bản thành giọng nói tự nhiên, nhiều giọng & ngôn ngữ.", icon: "wave", color: C.teal, tile: "rgba(80,209,170,.16)" },
  { key: "flows", title: "Flows", desc: "Thiết kế quy trình xử lý tự động cho video & âm thanh.", icon: "flows", color: C.blue, tile: "rgba(101,176,246,.16)" },
  { key: "runs", title: "Runs", desc: "Theo dõi tiến trình & lịch sử các tác vụ đã chạy.", icon: "runs", color: C.orange, tile: "rgba(255,181,114,.16)" },
  { key: "settings", title: "Cài đặt", desc: "Hồ sơ, giao diện, bảo mật & thông tin ứng dụng.", icon: "settings", color: C.steel, tile: "rgba(171,187,194,.14)" },
  { key: "api", title: "API", desc: "Tích hợp Beesoft Studio vào quy trình của bạn qua API.", icon: "api", color: C.coral, tile: "rgba(234,124,105,.16)" },
];

const SECTION: React.CSSProperties = { fontSize: 11.5, fontWeight: 600, letterSpacing: ".08em", textTransform: "uppercase", color: C.muted3, marginBottom: 14 };

export function Home({ onOpenFeature, recents, onOpenProject }: Props) {
  return (
    <div style={{ height: "100%", overflowY: "auto" }}>
      <div style={{ maxWidth: 900, margin: "0 auto", padding: "64px 32px 56px" }}>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", textAlign: "center", marginBottom: 46 }}>
          <div style={{ width: 54, height: 54, borderRadius: "50%", background: "conic-gradient(from 180deg,#9288E0 0 50%,#2d2a44 50% 100%)", border: "2px solid #6f64c4", marginBottom: 18, boxShadow: "0 6px 24px rgba(146,136,224,.3)" }} />
          <h1 style={{ margin: 0, fontSize: 32, fontWeight: 700, letterSpacing: "-.02em" }}>Beesoft Studio</h1>
          <p style={{ margin: "10px 0 0", fontSize: 15, color: C.muted }}>Bộ công cụ giọng nói &amp; lồng tiếng video — chạy hoàn toàn trên máy.</p>
        </div>

        <div style={SECTION}>Tính năng</div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 14 }}>
          {FEATURES.map((f) => {
            const isDub = f.key === "dub";
            return (
              <HoverBox
                key={f.key}
                onClick={() => onOpenFeature(f.key)}
                style={{
                  background: isDub ? "linear-gradient(160deg,rgba(146,136,224,.12),#1F1D2B)" : C.card,
                  border: `1px solid ${isDub ? "rgba(146,136,224,.4)" : C.borderSoft}`,
                  borderRadius: 13, padding: 20, cursor: "pointer", transition: "all .14s",
                }}
                hoverStyle={isDub ? { transform: "translateY(-2px)", borderColor: "rgba(146,136,224,.7)" } : { transform: "translateY(-2px)", borderColor: "#3a3d4c", background: C.cardHover }}
              >
                <div style={{ width: 42, height: 42, borderRadius: 11, background: f.tile, color: f.color, display: "grid", placeItems: "center", marginBottom: 14 }}>
                  <Icon name={f.icon} size={22} stroke={1.7} />
                </div>
                <div style={{ fontSize: 15.5, fontWeight: 700, marginBottom: 5 }}>{f.title}</div>
                <div style={{ fontSize: 12.5, color: C.muted, lineHeight: 1.5 }}>{f.desc}</div>
              </HoverBox>
            );
          })}
        </div>

        {recents.length > 0 && (
          <>
            <div style={{ ...SECTION, margin: "34px 0 14px" }}>Dự án gần đây</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {recents.map((r) => (
                <HoverBox
                  key={r.id}
                  onClick={() => onOpenProject(r.id)}
                  style={{ display: "flex", alignItems: "center", gap: 13, padding: "11px 14px 11px 11px", background: C.card, border: `1px solid ${C.borderSoft}`, borderRadius: 10, cursor: "pointer" }}
                  hoverStyle={{ borderColor: "#3a3d4c", background: C.cardHover }}
                >
                  <div style={{ width: 34, height: 44, flex: "none", borderRadius: 6, background: "linear-gradient(160deg,#222431,#15161d)", border: "1px solid #2f3140", display: "grid", placeItems: "center" }}>
                    <svg viewBox="0 0 24 24" width={15} height={15} fill="none" stroke={C.faint} strokeWidth={1.6}>
                      <path d="M5 4.5h14v15H5z" />
                      <path d="m10 9 6 3.5-6 3.5z" fill={C.faint} stroke="none" />
                    </svg>
                  </div>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 13.5, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{r.title}</div>
                    <div style={{ fontSize: 11, color: C.muted3, fontFamily: MONO, marginTop: 2 }}>{r.dur} · {r.statusLabel}</div>
                  </div>
                  <Icon name="chevronR" size={16} stroke={2} color={C.faint} style={{ flex: "none" }} />
                </HoverBox>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
