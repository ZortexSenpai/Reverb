// Genshin Impact character themes. Each one is a regular palette (its
// `[data-theme]` block lives in globals.css) plus background art: the motif
// SVG shipped next to it, or — when someone drops one into
// assets/themes/genshin/characters/ — real character art of the same name.
// The same goes for the small accent peeking over the player bar
// (`<slug>-bar.svg`, overridable with `characters/<slug>-bar.png` etc.).

const MOTIFS = import.meta.glob<string>("../../assets/themes/genshin/*.svg", {
  eager: true,
  query: "?url",
  import: "default",
});

const CHARACTERS = import.meta.glob<string>(
  "../../assets/themes/genshin/characters/*.{png,webp,jpg,jpeg,avif}",
  { eager: true, query: "?url", import: "default" },
);

/** File stem → URL, so lookups don't care about the folder or extension. */
function byStem(files: Record<string, string>): Map<string, string> {
  return new Map(
    Object.entries(files).map(([path, url]) => [
      path.slice(path.lastIndexOf("/") + 1, path.lastIndexOf(".")),
      url,
    ]),
  );
}

const motifUrls = byStem(MOTIFS);
const characterUrls = byStem(CHARACTERS);

export interface GenshinTheme {
  /** Theme value, always `genshin-<slug>`. */
  value: string;
  label: string;
  /** Names both the motif SVG and the optional character image. */
  slug: string;
  light?: boolean;
}

export const GENSHIN_THEMES: GenshinTheme[] = [
  { value: "genshin-citlali", label: "Citlali", slug: "citlali", light: true },
  { value: "genshin-skirk", label: "Skirk", slug: "skirk" },
  { value: "genshin-hu-tao", label: "Hu Tao", slug: "hu-tao" },
  { value: "genshin-raiden-shogun", label: "Raiden Shogun", slug: "raiden-shogun" },
  { value: "genshin-furina", label: "Furina", slug: "furina" },
  { value: "genshin-ganyu", label: "Ganyu", slug: "ganyu", light: true },
  { value: "genshin-mona", label: "Mona", slug: "mona" },
  { value: "genshin-lynette", label: "Lynette", slug: "lynette" },
  { value: "genshin-layla", label: "Layla", slug: "layla" },
  { value: "genshin-xiangling", label: "Xiangling", slug: "xiangling" },
  { value: "genshin-yaoyao", label: "Yaoyao", slug: "yaoyao", light: true },
  { value: "genshin-amber", label: "Amber", slug: "amber", light: true },
  { value: "genshin-xilonen", label: "Xilonen", slug: "xilonen" },
  { value: "genshin-escoffier", label: "Escoffier", slug: "escoffier" },
  { value: "genshin-lauma", label: "Lauma", slug: "lauma" },
];

export interface ThemeArt {
  url: string;
  /** Real character art rather than the bundled motif. */
  character: boolean;
}

/** The background art for a theme, or null for themes without any. */
export function themeArt(theme: string): ThemeArt | null {
  return resolveArt(theme, "");
}

/** Resolve `<slug><suffix>`: a dropped-in image first, then the bundled SVG. */
function resolveArt(theme: string, suffix: string): ThemeArt | null {
  const entry = GENSHIN_THEMES.find((t) => t.value === theme);
  if (!entry) return null;
  const stem = entry.slug + suffix;
  const character = characterUrls.get(stem);
  if (character) return { url: character, character: true };
  const motif = motifUrls.get(stem);
  return motif ? { url: motif, character: false } : null;
}

/** The small accent image that sits on the player bar, or null. */
export function playerBarAccent(theme: string): ThemeArt | null {
  return resolveArt(theme, "-bar");
}

