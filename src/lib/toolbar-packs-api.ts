/**
 * The bridge to the toolbars folder.
 *
 * Packs live beside the application's other data, one JSON file each, so the
 * reader can install one from the View menu or drop a file into the folder
 * later. Only the desktop build has that folder; the browser preview reads as
 * empty and says why.
 */

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { isDesktop } from "@/lib/errors";
import {
  isBuiltinToolbarId,
  normalizeToolbarPack,
  type RawToolbarPack,
  type ToolbarPack,
} from "@/lib/toolbar-packs";

export interface ToolbarPackProblem {
  /** The file that could not be read, or where the problem came from. */
  file: string;
  message: string;
}

export interface ToolbarPacksSnapshot {
  /** The folder packs are read from. Empty in the browser preview. */
  folder: string;
  /** Installed packs, normalized for the window. */
  packs: ToolbarPack[];
  /** Files that were found but could not be used, with the reason. */
  problems: ToolbarPackProblem[];
}

/** How an install or a removal failed, in words the dialog can show. */
export interface ToolbarPackFailure {
  kind: "exists" | "invalid" | "missing" | "builtin" | "io";
  message: string;
  /** The pack that is already installed, when `kind` is "exists". */
  name?: string;
}

interface RawSnapshot {
  folder: string;
  packs: RawToolbarPack[];
  problems: ToolbarPackProblem[];
}

const EMPTY: ToolbarPacksSnapshot = { folder: "", packs: [], problems: [] };

function desktopOnly(kind: ToolbarPackFailure["kind"] = "invalid"): ToolbarPackFailure {
  return {
    kind,
    message: "Toolbars can only be installed in the desktop app.",
  };
}

/** Read the packs folder. A broken file becomes a problem, not a failure. */
export async function listToolbarPacks(): Promise<ToolbarPacksSnapshot> {
  if (!isDesktop()) {
    return { ...EMPTY, packs: [], problems: [] };
  }

  const raw = await invoke<RawSnapshot>("list_toolbar_packs");
  const packs: ToolbarPack[] = [];
  const problems = [...raw.problems];
  const seen = new Set<string>();

  for (const entry of raw.packs) {
    const pack = normalizeToolbarPack(entry);
    if (!pack) {
      problems.push({
        file: `${entry.id || "toolbar"}.json`,
        message: "The toolbar file has a name or an icon the window cannot use.",
      });
      continue;
    }
    // A pack may not shadow a bar that ships with the application.
    if (isBuiltinToolbarId(pack.id) || seen.has(pack.id)) {
      problems.push({
        file: `${pack.id}.json`,
        message: "This toolbar reuses a name that is already taken.",
      });
      continue;
    }
    seen.add(pack.id);
    packs.push(pack);
  }

  packs.sort((a, b) => a.name.localeCompare(b.name));
  return { folder: raw.folder, packs, problems };
}

/** The system file picker, filtered to toolbar files. Null on cancel. */
export async function chooseToolbarPackFile(): Promise<string | null> {
  if (!isDesktop()) {
    return null;
  }
  const picked = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "Toolbar file", extensions: ["json"] }],
  });
  return typeof picked === "string" ? picked : null;
}

/**
 * Install a pack from a file. `replace` is the answer to the dialog's
 * confirmation when a pack with the same id is already there.
 */
export async function installToolbarPack(
  path: string,
  replace = false,
): Promise<ToolbarPack> {
  if (!isDesktop()) {
    throw desktopOnly();
  }
  const raw = await invoke<RawToolbarPack>("install_toolbar_pack", {
    path,
    replace,
  });
  const pack = normalizeToolbarPack(raw);
  if (!pack) {
    throw {
      kind: "invalid",
      message: "The toolbar file was accepted, but its icons could not be read.",
    } satisfies ToolbarPackFailure;
  }
  return pack;
}

export async function removeToolbarPack(id: string): Promise<void> {
  if (!isDesktop()) {
    throw desktopOnly();
  }
  await invoke<void>("remove_toolbar_pack", { id });
}

/** Reveal the folder, creating it first when this is the first visit. */
export async function revealToolbarPacks(): Promise<string> {
  if (!isDesktop()) {
    throw desktopOnly();
  }
  return invoke<string>("reveal_toolbar_packs");
}

/** Turn whatever the backend refused with into a failure the dialog knows. */
export function describeToolbarPackError(error: unknown): ToolbarPackFailure {
  if (typeof error === "object" && error !== null && "message" in error) {
    const candidate = error as { kind?: unknown; message?: unknown; name?: unknown };
    if (typeof candidate.message === "string") {
      return {
        kind:
          candidate.kind === "exists" ||
          candidate.kind === "missing" ||
          candidate.kind === "builtin" ||
          candidate.kind === "io"
            ? candidate.kind
            : "invalid",
        message: candidate.message,
        ...(typeof candidate.name === "string" ? { name: candidate.name } : {}),
      };
    }
  }
  if (typeof error === "string") {
    return { kind: "invalid", message: error };
  }
  return { kind: "io", message: "The toolbars folder did not answer." };
}
