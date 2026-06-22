// Platform helpers. In the Tauri app these call native commands/dialogs; in a
// plain browser they degrade gracefully (returning null) so the UI still works.

export function isTauri(): boolean {
  return typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== "undefined";
}

async function invokeSafe<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!isTauri()) return null;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<T>(cmd, args);
  } catch {
    return null;
  }
}

export async function serverBase(): Promise<string | null> {
  return invokeSafe<string>("server_base");
}

export async function studioBase(): Promise<string | null> {
  return invokeSafe<string>("studio_base");
}

export async function defaultOutputDir(): Promise<string | null> {
  return invokeSafe<string>("default_output_dir");
}

export type FfmpegStatus = { available: boolean; bundled: boolean; system: boolean; downloadable: boolean };

export async function ffmpegStatus(): Promise<FfmpegStatus | null> {
  return invokeSafe<FfmpegStatus>("ffmpeg_status");
}

/** Download a static ffmpeg into the app dir (Tauri only). Throws on failure. */
export async function downloadFfmpeg(): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("download_ffmpeg");
}

/** Base URL of the media-ai sidecar (port chosen at runtime by the shell). */
export async function mediaAiBase(): Promise<string | null> {
  return invokeSafe<string>("media_ai_base");
}

/// Copy a server-generated file to a destination on disk (Tauri only).
/** Copy a file. Returns null on success, or an error message string on failure. */
export async function copyFile(src: string, dest: string): Promise<string | null> {
  if (!isTauri()) return "không hỗ trợ ngoài ứng dụng";
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("copy_file", { src, dest });
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}

/** Show a native message dialog (WKWebView lacks window.alert). */
export async function showMessage(message: string, title = "Thông báo", kind: "info" | "error" = "info"): Promise<void> {
  if (!isTauri()) return;
  try {
    const { message: dlg } = await import("@tauri-apps/plugin-dialog");
    await dlg(message, { title, kind });
  } catch {
    /* ignore */
  }
}

export async function pickDirectory(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const res = await open({ directory: true, multiple: false });
    return typeof res === "string" ? res : null;
  } catch {
    return null;
  }
}

export async function pickVideoFile(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const res = await open({
      directory: false,
      multiple: false,
      filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv", "webm", "m4v", "avi"] }],
    });
    return typeof res === "string" ? res : null;
  } catch {
    return null;
  }
}

export async function saveAsDialog(defaultPath: string): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    return (await save({ defaultPath, filters: [{ name: "Video", extensions: ["mp4"] }] })) ?? null;
  } catch {
    return null;
  }
}

/** The user's Desktop directory (for default save locations), or null on web. */
export async function desktopDir(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    const { desktopDir } = await import("@tauri-apps/api/path");
    return await desktopDir();
  } catch {
    return null;
  }
}

export async function revealInDir(path: string): Promise<void> {
  if (!isTauri()) return;
  try {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  } catch {
    /* ignore */
  }
}

/** Open a URL in the user's default browser (WKWebView can't open new windows). */
export async function openExternal(url: string): Promise<void> {
  if (isTauri()) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
      return;
    } catch {
      /* fall through to web */
    }
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export function joinPath(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}
