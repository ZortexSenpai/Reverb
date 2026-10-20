// Message formatting and locale resolution, deliberately free of React and of
// any store import: the app entry points live in ./index.ts, so this half stays
// importable from tests and from plain modules.
//
// Messages use a small ICU subset:
//   "Filter {kind}…"                         → {name} placeholder
//   "{n, plural, one {# song} other {# songs}}" → plural, `#` is the count
//   "{n, plural, =0 {nothing} other {# songs}}" → exact match wins over category
//
// Plural categories come from Intl.PluralRules, so French treating 0 as
// singular ("0 lecture") and German/Spanish treating it as plural all fall out
// of the same message. A category the message doesn't spell out falls back to
// `other`.
//
// Interpolated numbers are formatted for the locale, thousands separator and
// all. Anything that must NOT be grouped that way — a year, an id — should be
// passed as a string. Dates additionally carry the OS's region (see
// `formatLocale`), so day/month order follows the machine, not the language.

import type { Messages } from "./messages/en";
import { en } from "./messages/en";
import { de } from "./messages/de";
import { es } from "./messages/es";
import { fr } from "./messages/fr";

export type MessageKey = keyof Messages;

export const LOCALES = ["en", "fr", "es", "de"] as const;
export type Locale = (typeof LOCALES)[number];

/** Language names in their own language — the convention for a language picker. */
export const LANGUAGE_NAMES: Record<Locale, string> = {
  en: "English",
  fr: "Français",
  es: "Español",
  de: "Deutsch",
};

const CATALOGUES: Record<Locale, Messages> = { en, fr, es, de };

export type Values = Record<string, string | number>;

export interface T {
  /** The message for `key`, with `{placeholders}` and plurals resolved. */
  (key: MessageKey, values?: Values): string;
  locale: Locale;
  /** A number in this locale ("1,234" / "1 234" / "1.234"). */
  n(value: number, options?: Intl.NumberFormatOptions): string;
  /** A date in this locale. Accepts a Date, unix milliseconds, or an ISO string. */
  d(value: Date | number | string, options?: Intl.DateTimeFormatOptions): string;
}

function isLocale(value: string | undefined): value is Locale {
  return value != null && (LOCALES as readonly string[]).includes(value);
}

/**
 * The locale to render in, given the stored preference. Anything that is not a
 * language we ship — "system", or a value written by a later release — resolves
 * against the OS languages, then falls back to English.
 */
export function resolveLocale(preference: string | undefined): Locale {
  if (isLocale(preference)) return preference;
  const tags =
    typeof navigator === "undefined"
      ? []
      : (navigator.languages ?? (navigator.language ? [navigator.language] : []));
  for (const tag of tags) {
    const base = tag.split("-")[0].toLowerCase();
    if (isLocale(base)) return base;
  }
  return "en";
}

/**
 * The region the OS formats dates and numbers for, as a bare subtag ("CH").
 *
 * Only the region is taken: the *language* is the user's choice in settings, so
 * month names have to follow that, while day/month order and separators are a
 * machine convention. Without this, a display language of "en" resolves to
 * en-US and shows 8/28/2026 to someone whose system is Swiss.
 */
function osRegion(): string | undefined {
  if (typeof navigator === "undefined") return undefined;
  const tags = navigator.languages ?? (navigator.language ? [navigator.language] : []);
  for (const tag of tags) {
    // "de-CH", "en-Latn-GB", "es-419" — the script subtag is optional.
    const region = /^[A-Za-z]{2,3}(?:-[A-Za-z]{4})?-([A-Za-z]{2}|\d{3})\b/.exec(tag)?.[1];
    if (region) return region.toUpperCase();
  }
  return undefined;
}

const OS_REGION = osRegion();

/**
 * The locale to format dates with: the display language, carrying the OS's
 * regional conventions. Falls back to the bare language when the system does
 * not name a region.
 *
 * Numbers deliberately do not go through this. Grouping and the decimal mark
 * belong to the language the user chose to read — English "1,234.5" against
 * German "1.234,5" — and re-regionalising them would silently restyle every
 * count in the app for a request that was about dates.
 */
export function formatLocale(locale: Locale, region: string | undefined): string {
  return region ? `${locale}-${region}` : locale;
}

// Intl formatters are expensive to build and are reused constantly here.
const numberFormats = new Map<string, Intl.NumberFormat>();
const dateFormats = new Map<string, Intl.DateTimeFormat>();

function numberFormat(locale: Locale, options?: Intl.NumberFormatOptions) {
  const key = `${locale}|${options ? JSON.stringify(options) : ""}`;
  let format = numberFormats.get(key);
  if (!format) {
    format = new Intl.NumberFormat(locale, options);
    numberFormats.set(key, format);
  }
  return format;
}

function dateFormat(locale: Locale, options?: Intl.DateTimeFormatOptions) {
  const key = `${locale}|${options ? JSON.stringify(options) : ""}`;
  let format = dateFormats.get(key);
  if (!format) {
    format = new Intl.DateTimeFormat(formatLocale(locale, OS_REGION), options);
    dateFormats.set(key, format);
  }
  return format;
}

const pluralRules = new Map<Locale, Intl.PluralRules>();

function pluralCategory(locale: Locale, count: number): string {
  let rules = pluralRules.get(locale);
  if (!rules) {
    rules = new Intl.PluralRules(locale);
    pluralRules.set(locale, rules);
  }
  return rules.select(count);
}

/** Index of the `}` closing the `{` at `start`, or -1 when unbalanced. */
function matchBrace(source: string, start: number): number {
  let depth = 0;
  for (let i = start; i < source.length; i++) {
    if (source[i] === "{") depth++;
    else if (source[i] === "}" && --depth === 0) return i;
  }
  return -1;
}

/** `one {…} other {…}` → `{ one: "…", other: "…" }`. */
function parseOptions(source: string): Record<string, string> {
  const options: Record<string, string> = {};
  let i = 0;
  while (i < source.length) {
    const open = source.indexOf("{", i);
    if (open === -1) break;
    const close = matchBrace(source, open);
    if (close === -1) break;
    options[source.slice(i, open).trim()] = source.slice(open + 1, close);
    i = close + 1;
  }
  return options;
}

/**
 * `count` is the plural count of the branch being rendered, which is what `#`
 * stands for; it is undefined outside a plural, where `#` is a literal.
 */
function formatMessage(
  message: string,
  values: Values,
  locale: Locale,
  count?: number,
): string {
  let out = "";
  let i = 0;
  while (i < message.length) {
    const char = message[i];
    if (char === "#" && count != null) {
      out += numberFormat(locale).format(count);
      i++;
    } else if (char !== "{") {
      out += char;
      i++;
    } else {
      const close = matchBrace(message, i);
      // Unbalanced: emit the rest verbatim rather than dropping it.
      if (close === -1) return out + message.slice(i);
      out += formatArgument(message.slice(i + 1, close), values, locale);
      i = close + 1;
    }
  }
  return out;
}

function formatArgument(body: string, values: Values, locale: Locale): string {
  const comma = body.indexOf(",");
  if (comma === -1) {
    const value = values[body.trim()];
    if (value == null) return "";
    return typeof value === "number" ? numberFormat(locale).format(value) : value;
  }

  const name = body.slice(0, comma).trim();
  const rest = body.slice(comma + 1).trim();
  if (!rest.startsWith("plural")) return `{${body}}`;

  const options = parseOptions(rest.slice("plural".length).replace(/^\s*,\s*/, ""));
  const count = Number(values[name] ?? 0);
  const branch =
    options[`=${count}`] ?? options[pluralCategory(locale, count)] ?? options.other ?? "";
  return formatMessage(branch, values, locale, count);
}

function build(locale: Locale): T {
  const catalogue = CATALOGUES[locale];
  const t = ((key: MessageKey, values: Values = {}) => {
    // A key missing from a translation falls back to English rather than
    // rendering the raw key at the user.
    const message = catalogue[key] ?? en[key];
    if (message == null) return key;
    return formatMessage(message, values, locale);
  }) as T;

  t.locale = locale;
  t.n = (value, options) => numberFormat(locale, options).format(value);
  t.d = (value, options) =>
    dateFormat(locale, options).format(typeof value === "string" ? new Date(value) : value);
  return t;
}

const translators = new Map<Locale, T>();

/**
 * The translator for a locale. Cached, so the identity is stable — components
 * can use it in dependency arrays without re-running every render.
 */
export function translator(locale: Locale): T {
  let t = translators.get(locale);
  if (!t) {
    t = build(locale);
    translators.set(locale, t);
  }
  return t;
}
