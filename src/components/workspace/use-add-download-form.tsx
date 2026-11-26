//! The model of the Add URL dialog: what the address means, where the file
//! goes, and what starting it does.
//!
//! The dialog owns no state of its own. Everything a reader can change and
//! everything the engine answers lives here, so the view is a form over this
//! object and nothing else.

import { useEffect, useMemo, useRef, useState } from "react";

import { FileTypeIcon } from "@/components/icons/desktop-icons";
import type { MenuRow } from "@/components/shell/menu-popup";
import {
  categoryTargetFolder,
  detectCategory,
  fileNameFromUrl,
  formatSize,
  joinPath,
  safeFileName,
  sameAddress,
  splitPath,
  urlProblem,
  type CategoryName,
} from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { galleryDownload } from "@/lib/ipc-gallery";
import { isYoutubePage } from "@/lib/media";
import { grabberDownload } from "@/lib/grabber";
import { debridAskUnlock, debridServices } from "@/lib/ipc-debrid";
import {
  chooseFolder,
  defaultDestination,
  inspectDestination,
  type DestinationInfo,
} from "@/lib/ipc";
import {
  loadSettings,
  preferredFolder,
  saveLastFolder,
  saveSettings,
} from "@/lib/settings";
import {
  driveExports,
  driveExportEndpoint,
  driveFileEndpoint,
  parseDriveLink,
} from "@/lib/drive";
import { useDownloads } from "@/state/download-store";
import { useLinkInspection } from "@/components/workspace/use-link-inspection";
import { usePageVideo } from "@/components/workspace/use-page-video";

interface AddDownloadFormProps {
  open: boolean;
  onClose: () => void;
  /** An address to open with, from a link watched on the clipboard. */
  seedUrl?: string | null;
}

/** A link the browser extension handed over, while it is still this address. */
interface HandedOver {
  url: string;
  name: string;
  referer: string;
}

export function useAddDownloadForm({
  open,
  onClose,
  seedUrl,
}: AddDownloadFormProps) {
  const {
    addDownload,
    browserRequest,
    categories,
    clearBrowserRequest,
    downloads: allDownloads,
  } = useDownloads();
  const [url, setUrl] = useState("");
  const [handedOver, setHandedOver] = useState<HandedOver | null>(null);
  const [folder, setFolder] = useState("");
  // Null follows the file name; a pick holds the category the user chose.
  const [categoryOverride, setCategoryOverride] = useState<CategoryName | null>(
    null,
  );
  const [categoryAt, setCategoryAt] = useState<{ x: number; y: number } | null>(
    null,
  );
  // The files of the page's media list the reader unticked. Everything the
  // page names starts chosen, so only the edits are kept.
  const [mediaRemoved, setMediaRemoved] = useState<string[]>([]);
  const [debridAvailable, setDebridAvailable] = useState(false);
  const [debridUnlock, setDebridUnlock] = useState(false);
  const [destination, setDestination] = useState<DestinationInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  // A second click in the same tick reaches the command before React has
  // re-rendered the disabled button; this is the guard that holds.
  const running = useRef(false);
  // The address is the one thing the user came for, so it has the caret
  // whenever the dialog opens, including after a browser hand-off. The
  // attribute carries that to `Dialog`, which focuses it after `showModal`.
  const urlField = useRef<HTMLInputElement>(null);
  const folderButton = useRef<HTMLButtonElement>(null);
  const categoryButton = useRef<HTMLButtonElement>(null);
  // A folder the user picked is theirs until the dialog closes, so a category
  // folder never overwrites it. A category folder applied below does the same
  // for the slower default the engine answers with.
  const folderPicked = useRef(false);
  const categoryFolderApplied = useRef(false);
  const baseFolder = useRef("");
  // The save folder of the options window, or the Downloads folder of the
  // system when it has none. A category without a folder of its own saves to a
  // folder of this one named after it, so it is not the same thing as the
  // folder the form opened on.
  const [defaultFolder, setDefaultFolder] = useState("");
  // The add dialog's first opening says where the files land, because no
  // save folder has been chosen yet; it is shown from the settings value
  // read at this moment and retired as soon as it has been seen.
  const [firstRunNotice, setFirstRunNotice] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const settings = loadSettings();
    if (!settings.firstRunDone && !preferredFolder()) {
      setFirstRunNotice(
        settings.defaultFolder.trim()
          ? `New downloads go to ${settings.defaultFolder.trim()} until you pick another folder here.`
          : "No save folder yet: new downloads will go to your Downloads folder. Pick another one here to make it yours.",
      );
    }
    if (!settings.firstRunDone) {
      saveSettings({ ...settings, firstRunDone: true });
    }
  }, [open]);

  // Open on a fresh form, whatever the last one left behind. A link the
  // clipboard watch offered is the only thing it carries in.
  useEffect(() => {
    if (!open) {
      return;
    }
    setUrl(seedUrl?.trim() ?? "");
    setHandedOver(null);
    setFolder("");
    folderPicked.current = false;
    setFirstRunNotice(null);
    categoryFolderApplied.current = false;
    baseFolder.current = "";
    setCategoryOverride(null);
    setCategoryAt(null);
    setMediaRemoved([]);
    setDestination(null);
    setError(null);
    setTouched(false);
    setBusy(false);
  }, [open, seedUrl]);

  // Start from the folder of the last download, then from the folder of the
  // options window, and from the Downloads folder of the system when neither
  // has one. The last two are the default save folder: the one a category
  // without a folder of its own saves under.
  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
    const fallback = loadSettings().defaultFolder.trim();
    setDefaultFolder(fallback);
    const remembered = preferredFolder();
    if (remembered) {
      baseFolder.current = remembered;
      setFolder(remembered);
    }
    // Asked for even when the form opened on the last folder: without it a
    // category has no default save folder to put its own folder inside.
    defaultDestination()
      .then((answer) => {
        if (!active) {
          return;
        }
        const target = splitPath(answer).dir;
        if (target && target !== ".") {
          setDefaultFolder(target);
        }
        if (
          !remembered &&
          !folderPicked.current &&
          !categoryFolderApplied.current
        ) {
          baseFolder.current = target || ".";
          setFolder(target || ".");
        }
      })
      .catch(() => {
        // The dialog still works after the folder is picked when the engine
        // cannot answer, which is also what happens in a plain browser window.
      });
    return () => {
      active = false;
    };
  }, [open]);

  // A link the browser handed over opens this dialog with its address, its
  // name and the page it came from already in it.
  useEffect(() => {
    if (!open || !browserRequest) {
      return;
    }
    setUrl(browserRequest.url);
    setHandedOver({
      url: browserRequest.url,
      name: browserRequest.name,
      referer: browserRequest.referer,
    });
    setError(null);
    setTouched(false);
    clearBrowserRequest();
  }, [open, browserRequest, clearBrowserRequest]);

  // A magnet link or a .torrent file is a torrent job, not an HTTP download:
  // the dialog hands it to Add torrent instead of starting the wrong thing,
  // because a torrent's files are chosen before a byte is written.
  const address = url.trim();
  const lower = address.toLowerCase();
  const torrent =
    lower.startsWith("magnet:") ||
    (lower.endsWith(".torrent") &&
      (address.includes(":\\") || address.startsWith("/")));
  const problem = torrent ? null : urlProblem(address);
  const valid = problem === null && !torrent;
  // A hand-over only describes the link it carried, not whatever was pasted
  // over it.
  const handed = handedOver && sameAddress(handedOver.url, address) ? handedOver : null;
  const referer = handed?.referer ?? "";

  // A Drive address is a page, not a file: the export format decides which
  // endpoint carries the bytes, and that is the endpoint the dialog probes.
  const drive = useMemo(
    () => (open ? parseDriveLink(address) : null),
    [open, address],
  );
  const driveDocument =
    drive?.kind === "document" ? drive.document : null;
  const [driveExport, setDriveExport] = useState("pdf");
  const driveProbeAddress =
    drive?.kind === "document" && drive.id && driveDocument
      ? driveExportEndpoint(drive.id, driveDocument, driveExport)
      : drive?.kind === "file" && drive.id
        ? driveFileEndpoint(drive.id)
        : address;

  // The probe and the gallery check are advice about this address; the form
  // reads their answer rather than asking the server itself.
  const { probe, gallery } = useLinkInspection({
    open,
    address,
    probeAddress: driveProbeAddress,
    referer,
    editing: url,
  });

  // Each editor is offered its own export list, so a sheet never shows a
  // PowerPoint entry that was never there.
  useEffect(() => {
    if (!driveDocument) {
      return;
    }
    setDriveExport((current) => {
      const options = driveExports(driveDocument);
      return options.some((option) => option.value === current)
        ? current
        : options[0].value;
    });
  }, [driveDocument]);

  // A signed-in debrid service unlocks the host links it takes before the
  // download starts; the toggle asks for this one.
  useEffect(() => {
    setDebridUnlock(false);
    if (!open) {
      setDebridAvailable(false);
      return;
    }
    if (!isDesktop()) {
      return;
    }
    let active = true;
    void debridServices()
      .then((services) => {
        if (active) {
          setDebridAvailable(services.some((service) => service.signedIn));
        }
      })
      .catch(() => {
        // No debrid accounts, or the engine gave no answer; the toggle stays
        // hidden.
      });
    return () => {
      active = false;
    };
  }, [open]);

  const mediaChoice = (gallery?.media.files ?? [])
    .map((file) => file.url)
    .filter((url) => !mediaRemoved.includes(url));

  // The name and the category follow what was detected until the user picks a
  // category. Nothing here is typed.
  const suggestedName = valid ? safeFileName(fileNameFromUrl(address)) : "";
  const servedName =
    probe.kind === "ready" && probe.name ? safeFileName(probe.name) : null;
  const browserName = handed?.name ? safeFileName(handed.name) : null;
  // The browser's name wins: the extension saw the page's own label (Facebook
  // HD/SD, YouTube's quality) and the server's name is only the file's last
  // path segment, which drops it.
  const name = browserName ?? servedName ?? suggestedName;
  const category =
    categoryOverride ?? (name ? detectCategory(name, categories) : "General");
  const categoryIcon = categories.find(
    (entry) => entry.name === category,
  )?.icon;
  const target =
    folder.trim() && name.trim() ? joinPath(folder.trim(), name.trim()) : "";

  // A page that plays a video is read here, not in a window of its own: the
  // dialog draws the qualities, in the same list the panel shows.
  const videoPage =
    valid && (isYoutubePage(address) || Boolean(gallery?.media.video));
  const pageVideo = usePageVideo({
    open,
    address,
    referer,
    videoPage,
    folder,
    name,
    running,
    setBusy,
    setError,
    onStarted: (chosen) => {
      saveLastFolder(chosen);
      onClose();
    },
  });

  // A category starts the dialog in its own folder: the one set on it, or the
  // folder of that name under the default save folder. General and Other, and a
  // category before the default has answered, give back the remembered folder.
  useEffect(() => {
    if (!open || folderPicked.current) {
      return;
    }
    const chosen = categoryTargetFolder(categories, category, defaultFolder);
    categoryFolderApplied.current = chosen !== "";
    setFolder(chosen || baseFolder.current);
  }, [open, category, categories, defaultFolder]);

  // Say so when the exact link is already in the list, without refusing it:
  // a second copy is a legitimate choice.
  const duplicate = useMemo(
    () =>
      valid && allDownloads.some((item) => sameAddress(item.url, address)),
    [valid, allDownloads, address],
  );

  // Announce an existing file before the second click replaces it. The check
  // is a debounce, so the engine is not asked for every letter of the address.
  useEffect(() => {
    setDestination(null);
    if (!open || !isDesktop() || !target) {
      return;
    }
    let active = true;
    const timer = setTimeout(() => {
      inspectDestination(target)
        .then((answer) => {
          if (active) {
            setDestination(answer);
          }
        })
        .catch(() => {
          // A half-typed folder is not an error yet; Download checks again
          // and shows what the engine says.
        });
    }, 400);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [open, target]);

  function reset() {
    setUrl("");
    setHandedOver(null);
    setFolder("");
    setCategoryOverride(null);
    setCategoryAt(null);
    setDestination(null);
    setTouched(false);
    setError(null);
  }

  function close() {
    if (running.current) {
      return;
    }
    reset();
    onClose();
  }

  async function pickFolder() {
    if (!isDesktop()) {
      setError("Folders can only be chosen in the desktop app.");
      return;
    }
    try {
      const picked = await chooseFolder(folder.trim());
      if (picked) {
        folderPicked.current = true;
        setFolder(picked);
        setError(null);
      }
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  async function submit() {
    if (running.current) {
      return;
    }
    setTouched(true);
    setError(null);

    if (!valid) {
      // The field explains itself; do not say it twice.
      urlField.current?.focus();
      return;
    }
    // A page that plays a video is never downloaded as a file: the address is
    // a player, and the only download from it is a quality the list shows.
    if (videoPage) {
      if (pageVideo.rows.length > 0 && pageVideo.choice) {
        await pageVideo.download();
      } else if (!pageVideo.error) {
        setError(
          "The page's video is still being read. Try again when its qualities appear.",
        );
      }
      return;
    }
    const chosenName = name.trim();
    if (!chosenName) {
      setError("The file needs a name; the address did not give one.");
      return;
    }
    const chosenFolder = folder.trim();
    if (!chosenFolder) {
      setError("Choose a folder to save the file in.");
      folderButton.current?.focus();
      return;
    }
    if (!isDesktop()) {
      setError("Downloading works only in the desktop app.");
      return;
    }

    if (debridUnlock && address) {
      try {
        await debridAskUnlock(address);
      } catch {
        // The toggle is advice; losing it is not a reason to hold the start.
      }
    }

    running.current = true;
    setBusy(true);
    try {
      const outcome = await addDownload({
        url: address,
        fileName: chosenName,
        category,
        savePath: chosenFolder,
        referer: referer || undefined,
        driveExport: driveDocument ? driveExport : undefined,
      });
      // A dismissed destination question leaves the dialog as it was, with
      // the address and the folder still in it.
      if (outcome === "cancelled") {
        return;
      }
      if (outcome === "started") {
        saveLastFolder(chosenFolder);
      }
      onClose();
    } catch (failure) {
      setError(describeError(failure));
      // The engine is the authority on the destination, so it is asked again
      // after a refusal; the hint under the folder follows its answer.
      void inspectDestination(joinPath(chosenFolder, chosenName))
        .then(setDestination)
        .catch(() => undefined);
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  async function downloadGallery() {
    if (!gallery?.isGallery || running.current) {
      return;
    }
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      await galleryDownload(url.trim(), undefined, referer || undefined);
      onClose();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  // The files the page names are ordinary downloads: the same hand-over the
  // site grabber uses, so each one queues, resumes and categorises like any
  // other file.
  async function downloadMediaFiles() {
    const chosen = (gallery?.media.files ?? []).filter((file) =>
      mediaChoice.includes(file.url),
    );
    const target = folder.trim();
    if (running.current || chosen.length === 0) {
      return;
    }
    if (!target) {
      setError("Choose a folder to save the files in.");
      folderButton.current?.focus();
      return;
    }
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      const results = await grabberDownload({
        items: chosen.map((file) => ({
          url: file.url,
          name: file.name,
          category,
        })),
        folder: target,
        referer: referer || undefined,
      });
      const started = results.filter(
        (result) => result.status === "started" || result.status === "queued",
      ).length;
      if (started === 0) {
        setError(results[0]?.message || "No file could be started.");
        return;
      }
      saveLastFolder(target);
      onClose();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  function toggleMedia(url: string) {
    setMediaRemoved((current) =>
      current.includes(url)
        ? current.filter((entry) => entry !== url)
        : [...current, url],
    );
  }

  function toggleCategoryMenu() {
    const box = categoryButton.current?.getBoundingClientRect();
    setCategoryAt(
      categoryAt || !box ? null : { x: box.left, y: box.bottom + 4 },
    );
  }

  const categoryRows: MenuRow<CategoryName>[] = categories.map((entry) => ({
    command: entry.name,
    label: entry.name,
    radio: true,
    icon: (
      <FileTypeIcon
        category={entry.name}
        icon={entry.icon}
        className="size-3.5"
      />
    ),
  }));

  const showProblem = touched && problem !== null;
  const sizeLabel =
    probe.kind === "ready"
      ? probe.total === null
        ? "Unknown size"
        : formatSize(probe.total)
      : null;
  const resumeLabel =
    probe.kind === "ready"
      ? probe.resumable
        ? "Resumable"
        : "Not resumable"
      : null;
  const details = [
    probe.kind === "checking" ? "Checking link…" : sizeLabel,
    resumeLabel,
    duplicate ? "Already in your list" : null,
  ]
    .filter(Boolean)
    .join(" · ");


  return {
    url,
    setUrl,
    handedOver,
    setHandedOver,
    folder,
    setFolder,
    categoryOverride,
    setCategoryOverride,
    categoryAt,
    setCategoryAt,
    probe,
    gallery,
    debridAvailable,
    setDebridAvailable,
    debridUnlock,
    setDebridUnlock,
    destination,
    setDestination,
    error,
    setError,
    touched,
    setTouched,
    busy,
    setBusy,
    running,
    urlField,
    folderButton,
    categoryButton,
    folderPicked,
    categoryFolderApplied,
    baseFolder,
    defaultFolder,
    setDefaultFolder,
    firstRunNotice,
    setFirstRunNotice,
    address,
    lower,
    torrent,
    problem,
    valid,
    handed,
    referer,
    drive,
    driveDocument,
    driveExport,
    setDriveExport,
    driveProbeAddress,
    suggestedName,
    servedName,
    browserName,
    name,
    category,
    categoryIcon,
    target,
    duplicate,
    reset,
    close,
    pickFolder,
    submit,
    downloadGallery,
    media: gallery?.media ?? null,
    mediaChoice,
    toggleMedia,
    videoPage,
    video: pageVideo.view,
    qualities: pageVideo.rows,
    qualityChoice: pageVideo.choice,
    chooseQuality: pageVideo.choose,
    qualityError: pageVideo.error,
    downloadMediaFiles,
    toggleCategoryMenu,
    categoryRows,
    showProblem,
    sizeLabel,
    resumeLabel,
    details,
  };
}
