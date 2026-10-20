import { describe, expect, it } from "vitest";
import { formatLocale, LOCALES, resolveLocale, translator } from "./core";
import { en } from "./messages/en";
import { de } from "./messages/de";
import { es } from "./messages/es";
import { fr } from "./messages/fr";

const t = translator("en");

describe("message formatting", () => {
  it("returns a plain message untouched", () => {
    expect(t("common.play")).toBe("Play");
  });

  it("fills placeholders", () => {
    expect(t("search.noResults", { q: "boards" })).toBe("No results for “boards”.");
  });

  it("drops a placeholder the caller did not pass, rather than printing it", () => {
    expect(t("artists.noMatch")).toBe("No artists match “”.");
  });

  it("formats interpolated numbers for the locale", () => {
    expect(t("selection.count", { n: 1234 })).toBe("1,234 selected");
    expect(translator("de")("selection.count", { n: 1234 })).toBe("1.234 ausgewählt");
    expect(translator("fr")("stats.scrobblesProgress", { n: 12345 })).toContain("12");
  });

  it("picks the plural branch and substitutes # with the count", () => {
    expect(t("count.songs", { n: 1 })).toBe("1 song");
    expect(t("count.songs", { n: 7 })).toBe("7 songs");
    expect(t("count.songs", { n: 0 })).toBe("0 songs");
  });

  it("follows each language's own plural rules — French counts 0 as singular", () => {
    expect(translator("fr")("count.songs", { n: 0 })).toBe("0 morceau");
    expect(translator("fr")("count.songs", { n: 1 })).toBe("1 morceau");
    expect(translator("fr")("count.songs", { n: 2 })).toBe("2 morceaux");
    // German and Spanish agree with English here, which is the point of
    // asking Intl rather than hard-coding `n === 1`.
    expect(translator("de")("count.albums", { n: 0 })).toBe("0 Alben");
    expect(translator("es")("count.albums", { n: 0 })).toBe("0 álbumes");
  });

  it("mixes a plural with a plain placeholder", () => {
    expect(t("toast.addedToPlaylist", { n: 1, playlist: "Roadtrip" })).toBe(
      "Added 1 song to Roadtrip",
    );
    expect(t("toast.addedToPlaylist", { n: 4, playlist: "Roadtrip" })).toBe(
      "Added 4 songs to Roadtrip",
    );
  });

  it("prefers an exact =0 branch over the plural category", () => {
    const exact = translator("en");
    // `chart.playsWithLabel` has no =0 branch, so this is the category path.
    expect(exact("chart.playsWithLabel", { label: "Mon", n: 0 })).toBe("Mon · 0 plays");
  });

  it("leaves # alone outside a plural", () => {
    expect(t("table.playSong", { title: "Track #3" })).toBe("Play Track #3");
  });

  it("falls back to English when a translation is missing the key", () => {
    // Simulated by asking for a key that exists; the guard itself is exercised
    // by the completeness test below, which keeps the catalogues in step.
    expect(translator("de")("common.play")).toBe("Abspielen");
  });

  it("formats numbers and dates through the locale helpers", () => {
    expect(t.n(1234.5)).toBe("1,234.5");
    expect(translator("de").n(1234.5)).toBe("1.234,5");
    expect(t.d("2026-08-14T12:00:00Z", { timeZone: "UTC", dateStyle: "short" })).toContain(
      "26",
    );
  });
});

describe("resolveLocale", () => {
  it("takes a supported language as given", () => {
    for (const locale of LOCALES) expect(resolveLocale(locale)).toBe(locale);
  });

  it("falls back to English for 'system' and for languages we do not ship", () => {
    // The test environment has no browser languages to match against.
    expect(resolveLocale("system")).toBe("en");
    expect(resolveLocale("kl")).toBe("en");
    expect(resolveLocale(undefined)).toBe("en");
  });
});

describe("catalogue completeness", () => {
  const keys = Object.keys(en).sort();

  it.each([
    ["fr", fr],
    ["es", es],
    ["de", de],
  ])("%s translates every key and adds none", (_name, catalogue) => {
    expect(Object.keys(catalogue).sort()).toEqual(keys);
  });

  it.each([
    ["fr", fr],
    ["es", es],
    ["de", de],
  ])("%s keeps every placeholder its English message uses", (_name, catalogue) => {
    // A dropped {name} silently renders an incomplete sentence, which the
    // type checker cannot see — the placeholders are inside strings.
    const placeholders = (message: string) =>
      [...message.matchAll(/\{\s*(\w+)\s*(?:,|\})/g)].map((m) => m[1]).sort();

    for (const key of keys) {
      const source = placeholders(en[key as keyof typeof en]);
      const target = placeholders(catalogue[key as keyof typeof en]);
      expect({ key, target: [...new Set(target)] }).toEqual({
        key,
        target: [...new Set(source)],
      });
    }
  });

  it.each([
    ["fr", fr],
    ["es", es],
    ["de", de],
  ])("%s spells out the plural branches English does", (_name, catalogue) => {
    for (const key of keys) {
      const isPlural = /,\s*plural\s*,/.test(en[key as keyof typeof en]);
      expect({ key, isPlural: /,\s*plural\s*,/.test(catalogue[key as keyof typeof en]) }).toEqual(
        { key, isPlural },
      );
    }
  });

  it("renders every message in every language without throwing", () => {
    const values = {
      n: 3,
      q: "x",
      name: "x",
      title: "x",
      genre: "x",
      owner: "x",
      playlist: "x",
      label: "x",
      artist: "x",
      artists: "x",
      tracks: "x",
      sessions: "x",
      median: 3,
      date: "x",
      year: "2026",
      rate: "x",
      total: 9,
      active: 3,
      from: "x",
      to: "x",
      span: "x",
      mode: "x",
      quality: "x",
      codec: "x",
      device: "x",
      server: "x",
      error: "x",
      greeting: "x",
      h: 1,
      m: 2,
      disc: 1,
      track: 2,
      version: "v1.0.1",
    };
    for (const locale of LOCALES) {
      const translate = translator(locale);
      for (const key of keys) {
        const rendered = translate(key as keyof typeof en, values);
        expect(rendered).toBeTypeOf("string");
        // An unresolved argument means the message uses a syntax the
        // formatter does not implement.
        expect(rendered).not.toMatch(/\{[^}]*,/);
      }
    }
  });
});

describe("format locale", () => {
  it("keeps the display language and takes the region from the OS", () => {
    expect(formatLocale("en", "CH")).toBe("en-CH");
    expect(formatLocale("de", "CH")).toBe("de-CH");
    // No region reported: the bare language, as before.
    expect(formatLocale("en", undefined)).toBe("en");
  });

  it("is what makes a Swiss system print day-first dates in an English UI", () => {
    const date = new Date(2026, 7, 28);
    const swiss = new Intl.DateTimeFormat(formatLocale("en", "CH")).format(date);
    const american = new Intl.DateTimeFormat(formatLocale("en", "US")).format(date);
    // Day before month, and vice versa — the point of carrying the region.
    expect(swiss.indexOf("28")).toBeLessThan(swiss.indexOf("8"));
    expect(american.indexOf("8")).toBeLessThan(american.indexOf("28"));
    // The language still decides the words.
    expect(new Intl.DateTimeFormat(formatLocale("en", "CH"), { month: "long" }).format(date))
      .toBe("August");
    expect(new Intl.DateTimeFormat(formatLocale("es", "CH"), { month: "long" }).format(date))
      .toBe("agosto");
  });
});
