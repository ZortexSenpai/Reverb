---
status: accepted
---

# HQPlayer fetches Songs from Reverb, not from Navidrome

When a device config plays through HQPlayer, Reverb tells HQPlayer which URL to play and HQPlayer fetches it itself. That URL points at a small HTTP relay inside Reverb, which forwards each request (Range included, for seeking) to Navidrome. Handing HQPlayer Navidrome's own stream URL would have been less code, but that URL carries the account's token in its query string, which HQPlayer would display and keep in its playlist. It ends in `/rest/stream` rather than in the file type, and HQPlayer reports every such URL back as the same one, query stripped. It often names an address only Reverb's computer can reach, such as `localhost`, a VPN, or a reverse proxy behind a login. The relay serves each Song under an unguessable path ending in its file extension, serves nothing it was not asked to, and listens only while an HQPlayer device config is in use.

## Consequences

HQPlayer has to be able to reach Reverb's computer, so a firewall between the two has to let it through. Reverb plays no part in the sound: HQPlayer decodes the original file, which also means formats Reverb's own engine cannot decode (DSD) play fine, and Reverb's EQ, ReplayGain and crossfade do not apply. If those are ever wanted on the HQPlayer path, the relay is the place to put a decoder in front of them, the way some players send HQPlayer PCM they have processed themselves. That same decoder would also replace today's workaround for M4A (ALAC, AAC), which HQPlayer cannot decode and which is fetched from Navidrome as MP3 instead. Whatever the relay serves, it never sends a chunked response: HQPlayer would read the chunk headers as audio.
