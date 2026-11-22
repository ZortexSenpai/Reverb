---
status: accepted
---

# The Stats page reads one account, chosen in settings

Scrobbling and stats each have an account of their own, Last.fm or ListenBrainz. By default the Stats page follows the Scrobbling account; turning that off gives it a separate one: another service, another account, or another ListenBrainz server. Stats never merge two accounts. The same Plays usually reach every service a listener uses, through Navidrome forwarding to each or a client scrobbling to both, so a merged history would count most listening twice. Telling the copies apart is guesswork: the services store slightly different timestamps and spellings for the same Play, and a merge that dropped near-duplicates would also drop real repeats.

The two roles are separate because they often want different things. Plenty of scrobble servers (Maloja, Koito, multi-scrobbler) accept ListenBrainz submissions but hand no history back, so a listener who scrobbles to one still needs listenbrainz.org or Last.fm for stats.

## Consequences

Only one imported history is kept: `stats/<service>.jsonl` plus its sync cursor in `sources.json`, belonging to one service, account and server. When the Stats source becomes a different account, that history is deleted and the new one imported from scratch. When there is briefly no Stats source, for example while switching "use the scrobbling account" off before connecting another, the history stays untouched, so switching back resumes instead of re-importing. Disconnecting the Scrobbling account hands it to stats when stats were following it, so dropping scrobbling never takes the imported history with it. If a merged view is ever wanted, it needs a deliberate deduplication rule rather than a concatenation.
