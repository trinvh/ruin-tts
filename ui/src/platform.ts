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

/// Copy a server-generated file to a destination on disk (Tauri only).
export async function copyFile(src: string, dest: string): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("copy_file", { src, dest });
    return true;
  } catch {
    return false;
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

export async function saveAsDialog(defaultPath: string): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    return (await save({ defaultPath })) ?? null;
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

export function joinPath(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}
