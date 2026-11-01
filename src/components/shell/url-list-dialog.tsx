import { useId, useState } from "react";

import { RuleList } from "@/components/settings/rule-list";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import {
  DEFAULT_URL_PROTOCOLS,
  MAX_URL_PROTOCOLS,
  cleanUrlProtocol,
  normalizeUrlProtocols,
} from "@/lib/tray";

interface UrlListDialogProps {
  /** The protocols the clipboard watch offers a link for, as saved. */
  protocols: readonly string[];
  onSave: (protocols: string[]) => void;
  onClose: () => void;
}

/**
 * The URL list of the reference product: what a copied address has to start
 * with to be worth offering as a download.
 *
 * The list holds protocols, not sites — `https` covers any secure address,
 * `mms` a streaming one. What the browser hands over is a different choice, on
 * Settings > Browser.
 */
export function UrlListDialog({ protocols, onSave, onClose }: UrlListDialogProps) {
  const fieldId = useId();
  // The draft starts in the shape the backend stores, so what is on the screen
  // is what the clipboard watch will match.
  const [draft, setDraft] = useState<string[]>(() => normalizeUrlProtocols(protocols));
  const unchanged = sameList(draft, normalizeUrlProtocols(protocols));

  return (
    <Dialog
      open
      title="Customize URL List"
      onClose={onClose}
      className="w-[520px]"
      footer={
        <>
          <DialogAction
            tone="quiet"
            className="mr-auto"
            onClick={() => setDraft([...DEFAULT_URL_PROTOCOLS])}
            disabled={sameList(draft, DEFAULT_URL_PROTOCOLS)}
          >
            Reset
          </DialogAction>
          <DialogAction onClick={onClose}>Cancel</DialogAction>
          <DialogAction
            tone="primary"
            data-autofocus
            disabled={unchanged}
            onClick={() => {
              onSave(draft);
              onClose();
            }}
          >
            OK
          </DialogAction>
        </>
      }
    >
      <p className="pb-3 text-xs text-chrome-muted">
        Offer a copied address as a download only when it starts with one of
        these protocols. The clipboard watch is the one that asks; it is turned
        on and off in Settings &gt; General.
      </p>

      <RuleList
        id={fieldId}
        label="Protocol"
        placeholder="https"
        values={draft}
        onChange={setDraft}
        clean={cleanUrlProtocol}
        format={(value) => `${value}://`}
        emptyText="Nothing is on the list, so nothing is offered. Save it empty and the three defaults come back."
        invalidText="A protocol is one word, like http, https, ftp or mms."
        max={MAX_URL_PROTOCOLS}
      />
    </Dialog>
  );
}

/** Two lists are the same when they hold the same protocols in the same order. */
function sameList(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}
