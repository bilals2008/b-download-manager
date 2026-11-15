/**
 * The language the window is in.
 *
 * The choice is kept in localStorage like the other window preferences, and
 * the direction of the window follows it: an Arabic window reads right to
 * left and the whole interface mirrors.
 */

import { useSyncExternalStore } from "react";

import {
  languageEntry,
  LANGUAGES,
  readLanguage,
  translate,
  type Language,
  type StringKey,
} from "@/i18n/catalogue";

export { LANGUAGES, type Language };

const STORAGE_KEY = "idm-clone.language";

const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function snapshot(): Language {
  return readLanguage();
}

/** Choose the language and mirror the window for Arabic. */
export function setLanguage(language: Language) {
  window.localStorage.setItem(STORAGE_KEY, language);
  const entry = languageEntry(language);
  document.documentElement.lang = language;
  document.documentElement.dir = entry.rtl ? "rtl" : "ltr";
  emit();
}

/** The language the window is in. */
export function useLanguage(): Language {
  return useSyncExternalStore(subscribe, snapshot, () => "en" as Language);
}

/** One word the app shows, in the chosen language. */
export function useT(): (key: StringKey) => string {
  const language = useLanguage();
  return (key: StringKey) => translate(language, key);
}

/** Apply the stored choice before the first paint. */
export function applyLanguage(): void {
  const language = readLanguage();
  const entry = languageEntry(language);
  document.documentElement.lang = language;
  document.documentElement.dir = entry.rtl ? "rtl" : "ltr";
}
