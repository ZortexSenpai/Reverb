// The sidebar's main navigation, shared with the setting that chooses which of
// these to show. Settings itself is deliberately not here: it is pinned to the
// bottom of the sidebar and always visible, so hiding everything else can
// always be undone.

import {
  BarChart3,
  Disc3,
  FolderTree,
  Heart,
  Home,
  ListMusic,
  Music,
  Search,
  Tags,
  Users,
  type LucideIcon,
} from "lucide-react";
import type { MessageKey } from "../../lib/i18n/core";

export interface NavItem {
  /** Route path, and the id persisted in `hiddenNavItems`. */
  to: string;
  labelKey: MessageKey;
  icon: LucideIcon;
}

export const NAV_ITEMS: NavItem[] = [
  { to: "/home", labelKey: "nav.home", icon: Home },
  { to: "/search", labelKey: "nav.search", icon: Search },
  { to: "/albums", labelKey: "nav.albums", icon: Disc3 },
  { to: "/songs", labelKey: "nav.songs", icon: Music },
  { to: "/artists", labelKey: "nav.artists", icon: Users },
  { to: "/genres", labelKey: "nav.genres", icon: Tags },
  { to: "/folders", labelKey: "nav.folders", icon: FolderTree },
  { to: "/playlists", labelKey: "nav.playlists", icon: ListMusic },
  { to: "/favourites", labelKey: "nav.favourites", icon: Heart },
  { to: "/stats", labelKey: "nav.stats", icon: BarChart3 },
];

/**
 * The items to show, given the hidden ids from preferences.
 *
 * Hiding is stored as a denylist so that navigation added in a later release
 * shows up for existing users instead of staying silently hidden, and so an id
 * that no longer exists fails towards "visible" rather than towards a nav item
 * nobody can reach.
 */
export function visibleNavItems(hidden: string[]): NavItem[] {
  return NAV_ITEMS.filter((item) => !hidden.includes(item.to));
}
