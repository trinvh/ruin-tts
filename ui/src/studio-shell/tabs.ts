import { C } from "./theme";
import type { IconName } from "./icons";

export type FeatureKey = "dub" | "tts" | "settings";
export type TabKind = "home" | FeatureKey | "project";

export interface Tab {
  id: string;
  kind: TabKind;
  /** project tabs carry the dub project id + display title */
  projectId?: string;
  title?: string;
}

/**
 * Features backed by an existing TanStack route (hybrid shell): clicking
 * navigates + shows <Outlet/>. Returns a literal route path (kept as a union so
 * the router's typed `navigate` accepts it) or null for overlay surfaces.
 */
export type RoutedPath = "/" | "/settings";

export function routeFor(key: FeatureKey): RoutedPath | null {
  switch (key) {
    case "tts":
      return "/";
    case "settings":
      return "/settings";
    default:
      return null; // dub / home / project are shell overlays
  }
}

export const FEATURE_TITLE: Record<FeatureKey, string> = {
  dub: "Lồng tiếng",
  tts: "Đọc (TTS)",
  settings: "Cài đặt",
};

export const FEATURE_ICON: Record<FeatureKey, IconName> = {
  dub: "film",
  tts: "wave",
  settings: "settings",
};

const DUB_LABEL: Record<string, string> = {
  created: "Mới tạo",
  extracting: "Đang tách tiếng…",
  extracted: "Đã tách tiếng",
  analyzing: "Đang phân tích…",
  analyzed: "Đã phân tích",
  translating: "Đang dịch…",
  translated: "Đã dịch",
  synthesizing: "Đang đọc…",
  synthesized: "Đã đọc",
  building: "Đang ghép…",
  built: "Đã ghép track",
  exporting: "Đang xuất…",
  done: "Đã xuất video",
  failed: "Lỗi",
  cancelled: "Đã huỷ",
};

export interface StatusStyle {
  label: string;
  fg: string;
  bg: string;
}

/** Map a dub project status string → pill label + colours (design palette). */
export function dubStatus(status: string): StatusStyle {
  const label = DUB_LABEL[status] ?? status;
  if (status === "failed") return { label, fg: C.pink, bg: "rgba(255,124,163,.16)" };
  if (status === "done") return { label, fg: C.teal, bg: "rgba(80,209,170,.16)" };
  if (status === "created") return { label, fg: C.ink2, bg: "rgba(124,128,148,.18)" };
  return { label, fg: C.purpleLt, bg: "rgba(146,136,224,.16)" };
}
