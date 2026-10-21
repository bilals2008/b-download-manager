import { useEffect, useState } from "react";

import { GeneralSettings } from "@/components/settings/general-settings";
import { SettingsNav } from "@/components/settings/settings-nav";
import { Dialog } from "@/components/ui/dialog";
import {
  DEFAULT_SETTINGS,
  loadSettings,
  saveSettings,
  type Settings,
} from "@/lib/settings";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * "Internet Download Manager Configuration" - the Options window. It opens on
 * the General page, keeps edits in a draft, and writes them only on Save so
 * Cancel genuinely discards.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const [draft, setDraft] = useState<Settings>(DEFAULT_SETTINGS);

  useEffect(() => {
    if (open) {
      setDraft(loadSettings());
    }
  }, [open]);

  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  function save() {
    saveSettings(draft);
    onClose();
  }

  return (
    <Dialog
      open={open}
      title="Internet Download Manager Configuration"
      onClose={onClose}
      className="h-[min(36rem,calc(100dvh-2rem))] w-[min(52rem,calc(100vw-2rem))] max-w-none"
      bodyClassName="flex p-0"
      footer={
        <>
          <span className="mr-auto hidden self-center text-chrome-micro text-chrome-muted sm:inline">
            Preferences stay on this device.
          </span>
          <button
            type="button"
            onClick={onClose}
            className="rounded-chrome border border-chrome-line px-3 py-1 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={save}
            className="rounded-chrome bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:opacity-90"
          >
            Save
          </button>
        </>
      }
    >
      <SettingsNav />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <GeneralSettings settings={draft} onChange={update} />
      </div>
    </Dialog>
  );
}
