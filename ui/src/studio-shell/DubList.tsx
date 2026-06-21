import { C, FONT, MONO } from "./theme";
import { Icon } from "./icons";
import { HoverBox } from "./ui";
import { dubStatus } from "./tabs";

export interface DubListItem {
  id: string;
  name: string;
  video_path: string;
  status: string;
}

interface Props {
  projects: DubListItem[];
  onOpen: (id: string) => void;
  onNew: () => void;
}

export function DubList({ projects, onOpen, onNew }: Props) {
  return (
    <div style={{ height: "100%", overflowY: "auto" }}>
      <div style={{ maxWidth: 1040, margin: "0 auto", padding: "34px 32px 48px" }}>
        <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 24, marginBottom: 28 }}>
          <div>
            <h1 style={{ margin: 0, fontSize: 28, fontWeight: 700, letterSpacing: "-.02em" }}>Lồng tiếng video</h1>
            <p style={{ margin: "8px 0 0", fontSize: 14, color: C.muted, lineHeight: 1.5, maxWidth: 560 }}>
              Nhập video tiếng Trung / Anh → tự tách giọng, dịch &amp; lồng tiếng Việt theo timestamp. Mỗi dự án mở trong một tab riêng.
            </p>
          </div>
          <HoverBox
            as="button"
            onClick={onNew}
            style={{ flex: "none", height: 40, padding: "0 18px", border: "none", background: C.purple, color: "#fff", borderRadius: 9, display: "flex", alignItems: "center", gap: 8, cursor: "pointer", fontFamily: FONT, fontSize: 14, fontWeight: 600, boxShadow: "0 4px 16px rgba(146,136,224,.35)" }}
            hoverStyle={{ background: "#9d93e8", boxShadow: "0 6px 22px rgba(146,136,224,.45)" }}
            activeStyle={{ transform: "scale(.98)" }}
          >
            <Icon name="plus" size={17} stroke={2.1} />
            Dự án mới
          </HoverBox>
        </div>

        {projects.length === 0 ? (
          <div style={{ border: `1px dashed ${C.border}`, borderRadius: 12, padding: "56px 24px", textAlign: "center", color: C.muted }}>
            <div style={{ fontSize: 14, lineHeight: 1.6 }}>
              Chưa có dự án. Bấm <b style={{ color: "#fff" }}>Dự án mới</b> và chọn một file video để bắt đầu.
            </div>
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            {projects.map((p) => {
              const st = dubStatus(p.status);
              return (
                <HoverBox
                  key={p.id}
                  onClick={() => onOpen(p.id)}
                  style={{ display: "flex", alignItems: "center", gap: 15, padding: "13px 16px 13px 13px", background: C.card, border: `1px solid ${C.borderSoft}`, borderRadius: 11, cursor: "pointer", transition: "all .14s" }}
                  hoverStyle={{ borderColor: "rgba(146,136,224,.5)", background: C.cardHover, transform: "translateY(-1px)" }}
                >
                  <div style={{ width: 46, height: 60, flex: "none", borderRadius: 7, background: "linear-gradient(160deg,#222431,#15161d)", border: "1px solid #2f3140", display: "grid", placeItems: "center", overflow: "hidden" }}>
                    <svg viewBox="0 0 24 24" width={20} height={20} fill="none" stroke={C.faint} strokeWidth={1.6}>
                      <path d="M5 4.5h14v15H5z" />
                      <path d="m10 9 6 3.5-6 3.5z" fill={C.faint} stroke="none" />
                    </svg>
                  </div>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 15, fontWeight: 600, color: "#fff", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", letterSpacing: "-.01em" }}>{p.name}</div>
                    <div style={{ fontSize: 12, color: C.muted3, fontFamily: MONO, marginTop: 3, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{p.video_path}</div>
                  </div>
                  <div style={{ flex: "none", display: "flex", alignItems: "center", gap: 14 }}>
                    <span style={{ padding: "5px 11px", borderRadius: 30, fontSize: 11.5, fontWeight: 600, color: st.fg, background: st.bg, whiteSpace: "nowrap" }}>{st.label}</span>
                    <Icon name="chevronR" size={17} stroke={2} color={C.faint} />
                  </div>
                </HoverBox>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
