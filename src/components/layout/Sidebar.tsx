import { useQuery } from "@tanstack/react-query";
import { ListMusic, Pin, Radio, Settings } from "lucide-react";
import { NavLink } from "react-router";
import { Logo } from "../ui/Logo";
import { UpdateMarker } from "../update/UpdateMarker";
import { visibleNavItems } from "./navItems";
import { useT } from "../../lib/i18n";
import { cmd } from "../../lib/ipc/commands";
import { usePlaylists } from "../../lib/subsonic/queries";
import { usePrefsStore } from "../../stores/prefsStore";

/**
 * The pinned "Last.fm only" switch. Hidden unless the user pinned it *and*
 * Last.fm scrobbling is still set up — the preference outlives the credentials
 * it depends on, and a switch that silently does nothing is worse than none.
 */
function LastfmOnlySwitch() {
  const t = useT();
  const pinned = usePrefsStore((s) => s.prefs.lastfmScrobbleOnlyPinned);
  const active = usePrefsStore((s) => s.prefs.lastfmScrobbleOnly);
  const set = usePrefsStore((s) => s.set);
  const { data: sources } = useQuery({
    queryKey: ["stats-sources"],
    queryFn: () => cmd("stats_get_sources"),
    enabled: pinned,
  });

  if (!pinned || !sources?.lastfm?.scrobbling) return null;

  return (
    <button
      role="switch"
      aria-checked={active}
      onClick={() => set({ lastfmScrobbleOnly: !active })}
      className={`glass-hover flex w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors ${
        active ? "text-text" : "text-dim hover:bg-surface-2/60 hover:text-text"
      }`}
    >
      <Radio className={`size-4 shrink-0 ${active ? "text-accent" : ""}`} />
      <span className="truncate">{t("sidebar.lastfmOnly")}</span>
      <span
        className={`relative ml-auto h-4 w-7 shrink-0 rounded-full transition-colors ${
          active ? "bg-accent" : "bg-surface-2"
        }`}
      >
        <span
          className={`absolute top-0.5 block size-3 rounded-full bg-text transition-transform ${
            active ? "translate-x-[14px]" : "translate-x-0.5"
          }`}
        />
      </span>
    </button>
  );
}

function PinnedPlaylists() {
  const t = useT();
  const pinnedIds = usePrefsStore((s) => s.prefs.pinnedPlaylists);
  const show = usePrefsStore((s) => s.prefs.showPinnedInSidebar);
  const { data: playlists } = usePlaylists(show && pinnedIds.length > 0);

  if (!show || pinnedIds.length === 0 || !playlists) return null;
  const pinned = pinnedIds
    .map((id) => playlists.find((p) => p.id === id))
    .filter((p): p is NonNullable<typeof p> => p != null);
  if (pinned.length === 0) return null;

  return (
    <>
      <div className="mt-4 mb-1 flex items-center gap-1.5 px-3 text-[10px] font-semibold tracking-wide text-faint uppercase">
        <Pin className="size-3" /> {t("sidebar.pinned")}
      </div>
      {pinned.map((playlist) => (
        <NavLink
          key={playlist.id}
          to={`/playlists/${playlist.id}`}
          className={({ isActive }) =>
            `flex items-center gap-3 rounded-md px-3 py-1.5 text-sm transition-colors ${
              isActive
                ? "glass-active bg-surface-2 font-medium text-text"
                : "glass-hover text-dim hover:bg-surface-2/60 hover:text-text"
            }`
          }
        >
          <ListMusic className="size-3.5 shrink-0 text-faint" />
          <span className="truncate">{playlist.name}</span>
        </NavLink>
      ))}
    </>
  );
}

export function Sidebar() {
  const t = useT();
  const liquid = usePrefsStore((s) => s.prefs.layout) === "liquid";
  const items = visibleNavItems(usePrefsStore((s) => s.prefs.hiddenNavItems));
  return (
    <nav
      className={`flex w-52 shrink-0 flex-col ${
        liquid ? "glass rounded-2xl" : "classic-panel border-r border-edge"
      }`}
    >
      <div className="flex items-center gap-2 px-4 pt-4 pb-6">
        <Logo className="size-7" />
        <span className="text-lg font-semibold tracking-tight">Reverb</span>
      </div>
      <div className="flex flex-1 flex-col gap-0.5 overflow-y-auto px-2">
        {items.map(({ to, labelKey, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors ${
                isActive
                  ? "glass-active bg-surface-2 font-medium text-text"
                  : "glass-hover text-dim hover:bg-surface-2/60 hover:text-text"
              }`
            }
          >
            <Icon className="size-4" />
            {t(labelKey)}
          </NavLink>
        ))}
        <PinnedPlaylists />
      </div>
      <div className="flex flex-col gap-0.5 px-2 pb-3">
        {/* The sidebar is in both layouts, so the marker is too. */}
        <UpdateMarker />
        <LastfmOnlySwitch />
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            `flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors ${
              isActive
                ? "glass-active bg-surface-2 font-medium text-text"
                : "glass-hover text-dim hover:bg-surface-2/60 hover:text-text"
            }`
          }
        >
          <Settings className="size-4" />
          {t("nav.settings")}
        </NavLink>
      </div>
    </nav>
  );
}
