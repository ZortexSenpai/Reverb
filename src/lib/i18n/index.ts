// React entry point for translations. The formatter itself lives in ./core,
// which imports no store — so tests (and prefsStore, which needs
// `resolveLocale`) can pull it in without dragging the app in with it.

import { usePrefsStore } from "../../stores/prefsStore";
import { resolveLocale, translator, type Locale, type T } from "./core";

export {
  LANGUAGE_NAMES,
  LOCALES,
  resolveLocale,
  translator,
  type Locale,
  type MessageKey,
  type T,
} from "./core";

/** The locale the UI is rendering in, resolved from the stored preference. */
export function useLocale(): Locale {
  return usePrefsStore((s) => resolveLocale(s.prefs.language));
}

/**
 * The translator for the current locale. Its identity only changes with the
 * language, so it is safe in dependency arrays.
 */
export function useT(): T {
  return translator(useLocale());
}

/** The current translator outside React — stores, event bridges, callbacks. */
export function getT(): T {
  return translator(resolveLocale(usePrefsStore.getState().prefs.language));
}
