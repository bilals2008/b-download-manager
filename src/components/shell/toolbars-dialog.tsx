import { useState } from "react";

import { Tooltip } from "@/components/ui/tooltip";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { describeError, isDesktop } from "@/lib/errors";
import { type ToolbarPack } from "@/lib/toolbar-packs";
import {
  chooseToolbarPackFile,
  describeToolbarPackError,
  installToolbarPack,
  removeToolbarPack,
  revealToolbarPacks,
  type ToolbarPacksSnapshot,
} from "@/lib/toolbar-packs-api";

interface ToolbarsDialogProps {
  snapshot: ToolbarPacksSnapshot;
  /** The pack in use, shown with a badge. */
  activeId: string;
  /** Read the folder again; resolves with what it now holds. */
  onRefresh: () => Promise<ToolbarPacksSnapshot>;
  /** Called after a pack is deleted, before the refreshed list arrives. */
  onPackRemoved: (id: string) => void;
  onClose: () => void;
}

/**
 * "Look for new toolbars" - the local shop. A toolbar is a JSON file: install
 * one from disk, drop one into the folder, or remove one. Nothing is fetched
 * from the network, so what is installed is exactly what the reader chose.
 */
export function ToolbarsDialog({
  snapshot,
  activeId,
  onRefresh,
  onPackRemoved,
  onClose,
}: ToolbarsDialogProps) {
  const [busy, setBusy] = useState<null | "install" | "remove" | "refresh">(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [replace, setReplace] = useState<{ path: string; name: string } | null>(null);
  const [removeId, setRemoveId] = useState<string | null>(null);

  const installed = snapshot.packs;

  async function chooseFile() {
    setError(null);
    setNotice(null);
    try {
      const path = await chooseToolbarPackFile();
      if (path) {
        await install(path, false);
      }
    } catch (failure) {
      setError(describeToolbarPackError(failure).message);
    }
  }

  async function install(path: string, replaceExisting: boolean) {
    setBusy("install");
    setError(null);
    setNotice(null);
    try {
      const pack = await installToolbarPack(path, replaceExisting);
      await onRefresh();
      setReplace(null);
      setNotice(`${pack.name} is installed.`);
    } catch (failure) {
      const refused = describeToolbarPackError(failure);
      if (refused.kind === "exists") {
        setReplace({ path, name: refused.name ?? refused.message });
      } else {
        setError(refused.message);
      }
    } finally {
      setBusy(null);
    }
  }

  async function remove(id: string) {
    setBusy("remove");
    setError(null);
    setNotice(null);
    try {
      await removeToolbarPack(id);
      onPackRemoved(id);
      await onRefresh();
      setRemoveId(null);
      setNotice("The toolbar was removed.");
    } catch (failure) {
      setError(describeToolbarPackError(failure).message);
    } finally {
      setBusy(null);
    }
  }

  async function rescan() {
    setBusy("refresh");
    setError(null);
    setNotice(null);
    try {
      const next = await onRefresh();
      setNotice(
        next.problems.length > 0
          ? `The folder was read again. ${next.problems.length} file(s) need attention.`
          : "The folder was read again.",
      );
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setBusy(null);
    }
  }

  async function openFolder() {
    setError(null);
    setNotice(null);
    try {
      await revealToolbarPacks();
    } catch (failure) {
      setError(describeToolbarPackError(failure).message);
    }
  }

  const locked = busy !== null;

  return (
    <Dialog
      open
      title="Look for new toolbars"
      onClose={onClose}
      className="max-h-[calc(100dvh-2rem)] w-[580px]"
      bodyClassName="max-h-[70vh] overflow-y-auto"
      footer={
        <>
          <DialogAction
            tone="quiet"
            className="mr-auto"
            onClick={openFolder}
            disabled={!isDesktop() || locked}
          >
            Open toolbars folder
          </DialogAction>
          <DialogAction tone="quiet" onClick={rescan} disabled={!isDesktop() || locked}>
            Rescan
          </DialogAction>
          <DialogAction onClick={onClose}>Close</DialogAction>
          <DialogAction
            tone="primary"
            onClick={chooseFile}
            disabled={!isDesktop() || locked}
          >
            Install from file...
          </DialogAction>
        </>
      }
    >
      <p className="text-xs text-chrome-muted">
        A toolbar pack is a JSON file that adds a style and may bring its own
        icons. Install one from a file you trust, or drop it into the folder
        below and choose Rescan.
      </p>
      <p className="mt-1.5 text-chrome-micro text-chrome-muted">
        Built-in styles are chosen from View &gt; Toolbar or Settings &gt;
        Toolbar. This window is for extra packs kept in the folder.
      </p>

      <div className="mt-3 flex items-center gap-2 text-chrome-micro text-chrome-muted">
        <span className="shrink-0">Folder</span>
        <Tooltip
          content={snapshot.folder || "Only the desktop app has a toolbars folder."}
          className="min-w-0 flex-1"
        >
          <span className="block truncate font-mono">
            {snapshot.folder || "Not available in the browser preview"}
          </span>
        </Tooltip>
      </div>

      {error ? (
        <p
          role="alert"
          className="mt-3 rounded-chrome border border-chrome-danger bg-chrome-danger px-2.5 py-2 text-xs text-chrome-danger-text"
        >
          {error}
        </p>
      ) : null}
      {notice ? (
        <p className="mt-3 rounded-chrome border border-chrome-line bg-chrome-menu-hover px-2.5 py-2 text-xs text-chrome-menu-text">
          {notice}
        </p>
      ) : null}

      <p className="mt-4 text-xs text-chrome-muted">Installed packs</p>
      {installed.length === 0 ? (
        <p className="mt-1 rounded-chrome border border-dashed border-chrome-line px-2.5 py-3 text-xs text-chrome-muted">
          No toolbar packs are installed yet. Use Install from file... to add
          one.
        </p>
      ) : (
        <ul className="mt-1 flex flex-col gap-1.5">
          {installed.map((pack) => (
            <PackRow
              key={pack.id}
              pack={pack}
              active={pack.id === activeId}
              action={
                removeId === pack.id ? (
                  <div className="flex shrink-0 items-center gap-1.5">
                    <DialogAction
                      tone="quiet"
                      onClick={() => setRemoveId(null)}
                      disabled={locked}
                    >
                      Keep
                    </DialogAction>
                    <DialogAction
                      tone="danger"
                      onClick={() => void remove(pack.id)}
                      disabled={locked}
                    >
                      Remove
                    </DialogAction>
                  </div>
                ) : (
                  <DialogAction
                    tone="quiet"
                    onClick={() => setRemoveId(pack.id)}
                    disabled={locked}
                  >
                    Remove
                  </DialogAction>
                )
              }
            />
          ))}
        </ul>
      )}

      {replace ? (
        <div className="mt-3 rounded-chrome border border-chrome-line bg-chrome-menu-hover px-2.5 py-2">
          <p className="text-xs text-chrome-menu-text">
            A toolbar with this name is already installed
            {replace.name ? `: ${replace.name}` : ""}. Replace it with the file
            you chose?
          </p>
          <div className="mt-2 flex justify-end gap-2">
            <DialogAction
              tone="quiet"
              onClick={() => setReplace(null)}
              disabled={locked}
            >
              Cancel
            </DialogAction>
            <DialogAction
              tone="danger"
              onClick={() => void install(replace.path, true)}
              disabled={locked}
            >
              Replace
            </DialogAction>
          </div>
        </div>
      ) : null}

      {snapshot.problems.length > 0 ? (
        <div className="mt-4">
          <p className="text-xs text-chrome-muted">Files that could not be used</p>
          <ul className="mt-1 max-h-28 overflow-y-auto rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 py-2 text-chrome-micro">
            {snapshot.problems.map((problem, index) => (
              <li key={`${problem.file}-${index}`} className="py-0.5">
                <span className="text-chrome-menu-text">{problem.file}</span>
                <span className="text-chrome-muted"> — {problem.message}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </Dialog>
  );
}

interface PackRowProps {
  pack: ToolbarPack;
  active: boolean;
  action?: React.ReactNode;
}

function PackRow({ pack, active, action }: PackRowProps) {
  return (
    <li className="flex items-start gap-2.5 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 py-2">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="truncate text-xs text-chrome-title-text">{pack.name}</span>
          {active ? <Badge>In use</Badge> : null}
        </div>
        {pack.description ? (
          <p className="mt-0.5 line-clamp-2 text-chrome-micro text-chrome-muted">
            {pack.description}
          </p>
        ) : null}
        {pack.author || pack.version ? (
          <p className="mt-0.5 text-chrome-micro text-chrome-muted">
            {[pack.author, pack.version].filter(Boolean).join(" · ")}
          </p>
        ) : null}
      </div>
      {action}
    </li>
  );
}

function Badge({ children }: { children: React.ReactNode }) {
  return (
    <span className="shrink-0 rounded-full bg-chrome-menu-active px-1.5 py-0.5 text-chrome-micro leading-none text-chrome-toolbar-text">
      {children}
    </span>
  );
}
