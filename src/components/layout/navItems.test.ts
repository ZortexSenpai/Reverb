import { describe, expect, it } from "vitest";
import { NAV_ITEMS, visibleNavItems } from "./navItems";

const routes = (hidden: string[]) => visibleNavItems(hidden).map((item) => item.to);

describe("visibleNavItems", () => {
  it("shows everything when nothing is hidden", () => {
    expect(visibleNavItems([])).toEqual(NAV_ITEMS);
  });

  it("drops the hidden routes and keeps the declared order", () => {
    expect(routes(["/songs", "/home"])).toEqual([
      "/search",
      "/albums",
      "/artists",
      "/genres",
      "/folders",
      "/playlists",
      "/favourites",
      "/stats",
    ]);
  });

  it("ignores ids that no longer exist, rather than hiding something else", () => {
    // A denylist written by an older release can name a route that has since
    // been renamed or removed; that must not disturb the rest of the nav.
    expect(routes(["/gone", "/albums"])).toEqual(
      NAV_ITEMS.filter((item) => item.to !== "/albums").map((item) => item.to),
    );
  });

  it("can hide every item — Settings is outside this list, so it stays reachable", () => {
    expect(visibleNavItems(NAV_ITEMS.map((item) => item.to))).toEqual([]);
  });
});
