# Reverb

A cross-platform (Linux / macOS / Windows) desktop music player for
[Navidrome](https://www.navidrome.org/) with a native, audiophile-grade audio
engine and a Roon-style **parametric EQ**.

## Features

- **Playback** — queue-based playing with play-now / play-next / add-to-queue,
  shuffle (upcoming-only, Spotify semantics), repeat off/all/one, gapless
  track transitions, sample-accurate seeking over HTTP ranges
- **Library** — albums (virtualized grid, all Subsonic sort orders), artists
  (A–Z with jump rail, top songs), genres (cover-art mosaics, drill into
  albums, play/shuffle a whole genre), playlists, favourites (songs / albums /
  artists), global search; text filters on every list view
- **Full-screen player** — ambient blurred-art backdrop, queue side panel with
  drag-reorder
- **Parametric EQ** — up to 10 bands (peak, low/high shelf, low/high pass),
  interactive frequency-response curve (drag handles, scroll for Q), named
  presets, auto/manual preamp headroom, click-free live adjustment while
  music plays
- **OS integration** — media keys via MPRIS (Linux), SMTC (Windows),
  Now Playing (macOS); scrobbling to Navidrome; queue restore across restarts
- **Settings** — output device selection, streaming quality (original /
  MP3 320/192/128 transcoding), ReplayGain (track/album, pre-amp, clip
  protection), scrobbling toggle, library rescan, accent color, startup view,
  grid density
- Keyboard shortcuts: `Space`, `←/→` seek, `Ctrl+←/→` prev/next, `F` full
  screen, `Ctrl+E` EQ, `M` mute, `S` shuffle, `R` repeat, `Ctrl+K` search

## Architecture

```
React 19 + TS UI (Tauri 2 webview)          src/
   │  typed commands / events
   ▼
Tauri shell: commands, reverb-art:// cover   src-tauri/
proxy+cache, keychain, media session
   │  crossbeam channels
   ▼
Native audio engine (no Tauri dependency)    crates/reverb-core/
   HTTP (reqwest) → symphonia decode →
   rubato resample → lock-free ring →
   [RT callback: biquad EQ chain → volume] → cpal
```

Key design points:

- The **frontend owns the queue**; the engine only knows the current track and
  one preloaded next track — which is how gapless works.
- All Navidrome HTTP happens in Rust (no CORS, credentials never enter the
  webview). Passwords live in the OS keychain.
- The EQ runs *inside the real-time audio callback* (RBJ biquads, f64 state,
  transposed direct form II) with wait-free coefficient publication, per-block
  smoothing for drags and an equal-power crossfade for bypass — no clicks.
  The TypeScript curve math mirrors the Rust DSP exactly, and both sides have
  matching unit tests.
- Tracks stream with `format=raw` (bit-perfect, seekable). Opus files fall
  back to server-side MP3 transcoding until symphonia ships an Opus decoder.

## Direct output on Linux (experimental)

Picking a `(direct)` output device addresses the DAC's ALSA card itself instead
of going through PipeWire or PulseAudio. That is the only way to get
bit-perfect output on Linux, and it means the card belongs to Reverb alone
while it plays.

Taking the card is only half of it: the sound server has to be told, or it goes
on trying to open a device it believes it still owns — failing, suspending, and
retrying every few seconds. Each attempt republishes the sink's volume, which
most desktops read as a volume change and answer with their volume OSD.

Reverb asks properly, over the `org.freedesktop.ReserveDevice1` D-Bus handshake
both servers implement. PipeWire's session manager reserves each card it
manages and yields to anything that asks for it with a higher priority, so it
tears its node down while Reverb holds the card and puts it back — default-sink
preference included — when Reverb lets go. Nothing outlives the process: the
claim is a D-Bus name, so it is released even if Reverb is killed outright.

Device reservation is on by default in a normal WirePlumber user session. To
check that the handshake is available:

```sh
# the session manager should own a name per ALSA card
busctl --user list | grep ReserveDevice1
# and each card should advertise the reservation it is under
pw-dump | grep -A2 'api.dbus.ReserveDevice1'
```

If nothing is listed, the feature is disabled — WirePlumber turns it off for
system-wide and embedded profiles (`support.reserve-device = disabled`). Enable
it for your session with a drop-in:

```sh
mkdir -p ~/.config/wireplumber/wireplumber.conf.d
cat > ~/.config/wireplumber/wireplumber.conf.d/50-reserve-device.conf <<'EOF'
wireplumber.profiles = {
  main = {
    support.reserve-device = required
    monitor.alsa.reserve-device = required
  }
}
EOF
systemctl --user restart wireplumber
```

Without the handshake, direct output still works, but the server keeps
competing for the card — expect the volume OSD to reappear every few seconds
and other applications to lose audio on that device. Pick **System default**
instead if that trade is not worth it.

## Development

Prerequisites: Rust (stable), Node 22+, and on Linux the Tauri system
dependencies:

```sh
# Fedora
sudo dnf install webkit2gtk4.1-devel librsvg2-devel alsa-lib-devel
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev libasound2-dev libgtk-3-dev
```

```sh
npm install
npm run tauri dev    # run the app
npx vitest run       # frontend unit tests (EQ curve math)
cargo test           # engine unit tests (DSP, models, auth)
npm run tauri build  # release bundles
```

A disposable test server is handy for development:

```sh
podman run -d --rm -p 4533:4533 -v /path/to/music:/music:ro docker.io/deluan/navidrome:latest
```

## License

GPL-3.0-only — see [LICENSE](LICENSE) for the full text.

Reverb bundles third-party components under permissive (MIT / Apache-2.0 /
BSD) and MPL-2.0 terms, which require their copyright notices and licence
texts to accompany any distributed build. Those are collected in
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md), and both files are
bundled with release builds. Regenerate the list after changing dependencies:

```sh
python3 scripts/gen-third-party.py
```
