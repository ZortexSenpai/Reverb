import { useQuery } from "@tanstack/react-query";
import { cmd } from "./ipc/commands";
import type { AccountStatus, ServiceId, ServicesStatus } from "./subsonic/types";

/** The settings sections for the two accounts, and a link to the stats one. */
export const SCROBBLING_SECTION_ID = "scrobbling";
export const STATS_SECTION_ID = "stats";
export const STATS_SETTINGS_PATH = `/settings#${STATS_SECTION_ID}`;

/** Every listening service, in the order they are offered. */
export const SERVICES: ServiceId[] = ["lastfm", "listenbrainz"];

/** The services' own spelling. Brand names, so not translated. */
export const SERVICE_NAMES: Record<ServiceId, string> = {
  lastfm: "Last.fm",
  listenbrainz: "ListenBrainz",
};

/** Shared by everything showing the accounts; imports and changes invalidate it. */
export const SERVICES_QUERY_KEY = ["services"] as const;

export function useServices({ enabled = true }: { enabled?: boolean } = {}) {
  return useQuery({
    queryKey: SERVICES_QUERY_KEY,
    queryFn: () => cmd("services_get"),
    enabled,
  });
}

/**
 * The account the Stats page imports from: the scrobbling account while stats
 * follow it, otherwise their own — and null when that account has no history
 * to give (a server that only accepts submissions), or there is none.
 */
export function statsReadsFrom(status: ServicesStatus | undefined): AccountStatus | null {
  const account = status?.statsSameAsScrobble ? status.scrobble : status?.statsAccount;
  return account?.history ? account : null;
}
