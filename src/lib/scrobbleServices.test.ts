import { describe, expect, it } from "vitest";
import { statsReadsFrom } from "./scrobbleServices";
import type { AccountStatus, ServicesStatus } from "./subsonic/types";

const lastfm: AccountStatus = { service: "lastfm", username: "u", history: true };
const listenbrainz: AccountStatus = { service: "listenbrainz", username: "lb", history: true };
const scrobbleOnlyServer: AccountStatus = {
  service: "listenbrainz",
  username: "",
  apiUrl: "https://maloja.example/apis/listenbrainz",
  history: false,
};

function status(patch: Partial<ServicesStatus>): ServicesStatus {
  return { statsSameAsScrobble: true, trackCount: 0, importing: false, ...patch };
}

describe("statsReadsFrom", () => {
  it("reads nothing before anything is connected", () => {
    expect(statsReadsFrom(undefined)).toBeNull();
    expect(statsReadsFrom(status({}))).toBeNull();
  });

  it("follows the scrobbling account by default", () => {
    expect(statsReadsFrom(status({ scrobble: lastfm, statsAccount: listenbrainz }))).toBe(lastfm);
  });

  it("reads its own account once it stops following", () => {
    const own = status({ statsSameAsScrobble: false, scrobble: lastfm, statsAccount: listenbrainz });
    expect(statsReadsFrom(own)).toBe(listenbrainz);
    expect(statsReadsFrom({ ...own, statsAccount: null })).toBeNull();
  });

  it("cannot follow a server that only accepts submissions", () => {
    expect(statsReadsFrom(status({ scrobble: scrobbleOnlyServer }))).toBeNull();
  });
});
