# ndiclient

Command line NDI receiver for Raspberry Pi OS (Raspbian) ARM64.
`ndiclient [-d] [--fullscreen] [--noaudio] [--ipaddr <ip>] [--wait-image <file.bmp>] [--retry-wait <seconds>] <device-name> [stream-name]`
uses NDI discovery to find the source named `DEVICE (Stream)` (or the first
stream on that device), shows its video in a desktop window or fullscreen
(or an optional image while no video arrives) and plays its audio. With
`--retry-wait` it connects anew when the video stays away too long.
`ndiclient [--ipaddr <ip>]` without names lists the sources it discovers.

## Coding standard

Follow Uncle Bob's "Clean Code" rules. In short:
- Keep it simple. Small functions that do one thing. Few arguments, no flag
  arguments, no side effects.
- Descriptive, searchable names. Named constants instead of magic numbers.
  Value objects instead of bare primitives.
- Keep configurable data at high levels. Encapsulate boundary conditions.
  Keep multithreading code separate. Use dependency injection.
- Comments explain intent or warn of consequences. No redundant comments
  and no commented-out code.
- Order code top-down. Keep related code close together and lines short.
- Tests: one assert per test. Tests should be fast, independent and repeatable.
- Leave code cleaner than you found it, and always fix root causes.

Documentation, comments, tests and commit messages never name the real
hosts, devices, NDI sources or IP addresses used in development or testing.
Use placeholders (`<hostname>`, `STUDIO`, `OBS PGM`) and the IP ranges
reserved for documentation (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24,
2001:db8::/32) instead.

## Usage

```sh
cargo build --release
target/release/ndiclient
target/release/ndiclient STUDIO "OBS PGM"
target/release/ndiclient studio.local "OBS PGM"
target/release/ndiclient STUDIO
target/release/ndiclient --fullscreen STUDIO "OBS PGM"
target/release/ndiclient --noaudio STUDIO
target/release/ndiclient -d STUDIO
target/release/ndiclient --ipaddr 192.0.2.20 STUDIO "OBS PGM"
target/release/ndiclient --ipaddr 192.0.2.20
target/release/ndiclient --wait-image standby.bmp STUDIO
target/release/ndiclient --retry-wait 30 STUDIO "OBS PGM"
packaging/build_deb.sh
```

- Without arguments: discovers for 5 s, prints each source's full NDI name
  (one per line, de-duplicated, sorted) and exits 0, also when none are found.
- `--fullscreen` (anywhere among the arguments) covers the whole screen
  instead of opening a 1280x720 resizable window. It is SDL "desktop"
  fullscreen: the display mode is not changed and the picture is
  letterboxed. Not valid without a device name.
- Audio plays on SDL's default output (PipeWire here) unless `--noaudio`
  is given. `--noaudio` opens no audio device, starts no audio thread and
  doesn't capture audio; it isn't valid without a device name either.
  "Playing audio: 48000 Hz, 2 channels" is printed when the output opens
  (again if the format changes). If no output can be opened, a warning is
  printed and the video still plays.
- `--ipaddr <ip>` (IPv4 or IPv6, no host names) asks that address for its
  sources directly (NDI `p_extra_ips`, unicast) besides mDNS, and only
  sources whose NDI URL address is that IP are used, for connecting and
  for listing. The connection stays at that address: if it breaks, the
  receiver waits for it to come back rather than move to another
  interface of the source. For multicast lost between Wi-Fi and wired
  Ethernet, and to pick one interface of a multi-homed source. Without a
  device name it lists the sources at that address. Options may come in any order, each
  at most once; `--ipaddr`, `--wait-image` and `--retry-wait` take the
  next argument as their value.
- `--wait-image <file.bmp>` shows that image whenever "Waiting for
  video..." is printed (also at start-up, after 3 s without a first
  frame), letterboxed to the window like the video, until the next new
  frame. BMP only (`SDL_LoadBMP`, so no SDL_image dependency); the file is
  read before discovery, and a missing or non-BMP file exits 1 at once.
  The image's texture exists only while it is shown. Not valid without a
  device name.
- `--retry-wait <seconds>` (whole seconds, 0 allowed): once "Waiting for
  video..." has been printed and that many more seconds pass without a
  frame, the connection is torn down (frame-sync, receiver, audio device
  and pump, and the finder) and made again as at start-up: "Reconnecting",
  a new finder searches for the source (10 s), then "Connecting to ...".
  The window stays open meanwhile, showing the last picture or the wait
  image, and still answers to Escape and F11/F. A search that finds
  nothing prints the "Did not find" message (it doesn't exit) and is
  repeated at once. Without a stream name, a reconnect may pick another
  stream on the device; the window title keeps the first one's name.
  Without the option, the stalled connection is waited for indefinitely
  (NDI's own reconnecting). Not found at start-up still exits 1. Not
  valid without a device name.
- Kiosk mode: in fullscreen the mouse cursor is hidden. F11 or F switches
  between windowed and fullscreen at runtime (held keys don't repeat it).
- Close the window or press Escape to quit (also in fullscreen).
- `-d` prints debug messages: the displayed frame rate every 5 seconds.
  Without it they are suppressed (the frame rate isn't even measured).
  Not valid without a device name.
- The source is the one whose full NDI name is `<device-name> (<stream-name>)`.
  The device name matches case-insensitively (it is a host name); the stream
  name matches exactly (case-sensitive). Without a stream name, the first
  stream discovered on the device is used.
- Gives up with exit code 1 if no matching source is found within 10 s; the
  error lists all source names discovered.
- The window opens black right after connecting. "Waiting for video..." is
  printed when 3 s pass without a frame, once per stall (also mid-stream).
- When run over SSH, export the desktop session first:
  `export XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=wayland-0`

## Environment (as developed)

- Raspberry Pi 5 (BCM2712, 4 GB), Debian 13 "trixie" based Raspberry Pi OS,
  kernel 6.18, labwc Wayland desktop.
- Rust 1.99 via rustup (user-local in `~/.cargo`), edition 2024.
- System package installed for this project: `libsdl2-dev` (SDL 2.32).
- NDI SDK v6.3.2 installed by its own installer (EULA accepted by the user) to
  `/home/pi/NDI SDK for Linux`. Nothing from it is installed system-wide.
  The ARM64 library is `lib/aarch64-rpi4-linux-gnueabi/libndi.so.6`.

## Build details

- `build.rs` links `libndi` from the SDK directory and embeds that directory as
  an rpath, so no `LD_LIBRARY_PATH` or system install is needed. Override the
  SDK location with the `NDI_SDK_DIR` environment variable at build time.
  `NDI_RUNTIME_DIR` replaces the rpath with another directory (used by the
  Debian package).
- `packaging/build_deb.sh` builds `target/debian/ndiclient_<version>_<arch>.deb`
  with plain `dpkg-deb` (no debhelper, no cargo-deb). It builds into its own
  target dir (`target/debian/cargo`) with `NDI_RUNTIME_DIR=/usr/lib/ndiclient`,
  stages `/usr/bin/ndiclient`, the NDI runtime in `/usr/lib/ndiclient` (the
  SDK docs say to keep NDI libraries in the application's folder, not the
  system path), docs/licenses and `changelog.Debian.gz` in
  `/usr/share/doc/ndiclient` and the man page `ndiclient.1.gz`, then fills
  `packaging/debian/control.in`. `Depends` comes from `dpkg-shlibdeps`
  (needs `dpkg-dev`); its warning about libndi linking libpthread is NDI's
  and harmless. The package version is the top entry of
  `packaging/debian/changelog` plus `+deb<VERSION_ID>` from
  `/etc/os-release` (`+deb13` on trixie, `+deb12` on bookworm), so builds
  for different Debian releases get distinct versions and file names, and
  a Debian upgrade upgrades the package; the build fails if its upstream part differs
  from Cargo.toml's version. Releases stamp both (see Distribution); a
  local build of a new version needs `packaging/stamp_release.sh`. Keep
  `packaging/debian/ndiclient.1` in sync with the usage.
- Distribution: `.github/workflows/apt-repository.yml` builds the package in
  a `debian:trixie` and a `debian:bookworm` container (a build matrix, one
  leg per Debian release; Raspberry Pi OS's current and previous release)
  on GitHub's `ubuntu-24.04-arm` runner. It
  downloads the NDI SDK from NDI's public URL and runs its installer with
  `y` piped in (the repository owner accepted the EULA), after checking the
  download against `NDI_SDK_SHA256`. NDI replaces that download in place,
  so a new SDK release fails the check until the checksum is updated. It
  runs when a GitHub release is published (or by hand on a release tag).
  The tag is the version, `X.Y.Z` without a `v`: a `release` job reads the
  release's notes and publish date with `gh`, and
  `packaging/stamp_release.sh` writes the version into Cargo.toml and
  Cargo.lock and adds a changelog entry (one item per non-blank line of
  the notes, Markdown list/heading markers removed; the maintainer from
  `control.in`) before building. It fails on a tag that isn't `X.Y.Z` or
  is older than the changelog, and adds no entry if the changelog already
  has that version. After publishing, a `commit` job stamps `main` the
  same way and pushes the result as the `github-actions[bot]` "Release
  X.Y.Z" commit (nothing if `main` already has it), so branch protection
  on `main` must let `GITHUB_TOKEN` push. Releases created with the
  workflow `GITHUB_TOKEN` don't trigger it (GitHub prevents that). Two jobs keep the signing key away
  from third-party code: `build` (NDI installer, cargo; read-only token, no
  secrets) uploads each `.deb` as artifact `debian-package-<distribution>`;
  `publish` waits for all legs, downloads them in a fresh trixie container,
  runs `packaging/update_apt_repo.sh` once per distribution on the
  `gh-pages` branch and pushes it; GitHub Pages serves it at
  https://deckerego.github.io/ndiclient (distributions `trixie` and
  `bookworm`, component `main`). Adding a Debian release means adding a
  matrix entry. Actions are pinned to commit SHAs and the `debian` images
  to digests; update the pins deliberately. `publish` uses the GitHub
  environment `publishing` (deployment rule: tags `*.*.*` only), which
  holds the secret `SIGNING_PRIVATE_KEY` (must have no passphrase) and the
  variable `SIGNING_PUBLIC_KEY` the signature is checked against. Keep
  them out of repository-level secrets and variables. `update_apt_repo.sh` keeps each distribution's
  packages in its own pool, `pool/<distribution>/main/`, so its index lists
  only those (before bookworm, trixie's were in `pool/main/`; the script
  moves them on its next trixie run). It keeps old packages and refuses to replace a `.deb` with different content, so a
  rebuild of one version needs a new changelog entry. It needs `apt-utils`
  and `gnupg`, the key id in `APT_SIGNING_KEY` and the repository's URL in
  `APT_REPO_URL`. It also writes `<distribution>.sources` to the
  repository root: a deb822 APT source (`Types`, `URIs`, `Suites`,
  `Components`, `Architectures` from the pool, so a foreign armhf
  architecture causes no warning) with the public key inline in
  `Signed-By`, so users install the repository with one `curl` to
  `/etc/apt/sources.list.d/ndiclient.sources`. `publish` checks each
  file's inline key against its distribution's `InRelease`. The keyring
  files (`ndiclient-archive-keyring.gpg`/`.asc`) are still exported for
  installs from the old one-line `ndiclient.list`.
- The `sdl2` crate is used with the `unsafe_textures` feature so a texture can
  live in the same struct as its `TextureCreator` (no self-referential lifetimes).
- NDI bindings are hand-written (`src/ndi/ffi.rs`), not bindgen, to avoid a
  libclang dependency. They cover only the functions used. Keep them in sync
  with the SDK headers in `include/` if the SDK is upgraded.

## Architecture

```
src/main.rs          argument parsing (Invocation, Command, ViewOptions:
                     -d, --fullscreen, --noaudio, --ipaddr, --wait-image,
                     --retry-wait), wiring, source listing, Viewer: the
                     window kept across connections, capture/display loop,
                     reconnecting
src/picture.rs       Picture / PixelLayout / Dimensions: display-agnostic frame view
src/sound.rs         Sound / AudioFormat: device-agnostic planar audio view,
                     interleaving, channel narrowing
src/padding.rs       PaddingRemover: drops the silence the frame-sync pads
                     its queue with once it has run dry
src/jitter_buffer.rs JitterBuffer / AudioLatency: audio reserve between the
                     frame-sync and the sound card (adaptive priming, faded
                     starts/stops/splices)
src/audio_pump.rs    the only threading code: thread moving audio from the
                     frame-sync into the jitter buffer
src/speaker.rs       Speaker: SDL audio output fed from the jitter buffer
src/i420.rs          I420Frame: repacks UYVY into planar I420 for the GPU
src/display.rs       VideoWindow / WindowMode: SDL2 window or fullscreen,
                     key handling, cursor hiding, streaming texture,
                     wait image, letterboxing
src/frame_rate.rs    FrameRateMeter: periodic fps reporting
src/stall.rs         StallDetector: "waiting for video" once per stall;
                     RetryPolicy: when a stall gives up the connection
src/wait_image.rs    WaitImage: the BMP shown while no video arrives
src/ndi/ffi.rs       raw C declarations for the NDI SDK
src/ndi/runtime.rs   NdiRuntime: RAII for NDIlib_initialize/NDIlib_destroy
src/ndi/query.rs     SourceQuery: device + optional stream name, matching rules
src/ndi/finder.rs    SourceFinder / SourceDescriptor / SourceSearch: list
                     sources, find one matching a query (blocking or polled)
src/ndi/scope.rs     DiscoveryScope: whole network or one IP address (--ipaddr)
src/ndi/receiver.rs  Receiver: the connection to the chosen source
src/ndi/frame_sync.rs FrameSync / VideoFrame / FrameStamp / AudioFrame: NDI
                     frame synchronizer, frames pulled at the local clocks
tools/               test-pattern NDI sender used for development (C)
packaging/           build_deb.sh, update_apt_repo.sh, stamp_release.sh;
                     Debian control template, copyright, changelog and
                     man page (packaging/debian/)
```

Key decisions:

- **Discovery by name.** The finder uses NDI's default discovery (mDNS, or a
  discovery server if configured in NDI's own config file); `p_extra_ips`
  only with `--ipaddr`.
  We pick the first source whose full name (`DEVICE (Stream)`) splits into the
  requested device and (if given) stream. "First" is NDI's list order, which
  is roughly discovery order, so with several streams on one device and no
  stream name the choice is not guaranteed to be stable between runs. NDI may list a source more than once; names are
  de-duplicated in errors.
- **Discovery scope.** A host with several interfaces (Wi-Fi and Ethernet)
  is announced once per interface, each with its own URL address, and
  multicast may get lost between them. `DiscoveryScope::Address` (from
  `--ipaddr`) queries that IP by unicast and makes the finder drop every
  source whose URL address (`<ip>:<port>`) is another IP, so `find_source`,
  listing and the error's source list only see that interface. The
  receiver uses that interface too (`Addressing::ByUrl`): its source has
  no name, only the URL. NDI ignores the URL whenever a name is given (as
  the SDK header says, and a probe confirmed: a name with an unreachable
  URL still connected) and resolves the name again on reconnect, so a
  receiver given both fell over to the source's Wi-Fi interface when its
  Ethernet link was pulled, in one of two instances. Without `--ipaddr`
  (`Addressing::ByName`) the name is kept, so NDI follows the source to
  any interface or new address. A pinned receiver reconnects when the
  sender returns on the same port (a sender that is alone on its host
  restarted on the same one), but not if it comes back on another.
  Earlier PoC versions required an IP; it became optional when discovery
  by name was added.
- **Black window on open.** Wayland maps a window only after its first
  present, so `VideoWindow::open` presents a cleared (black) frame.
- **Pixel format.** The receiver asks for `UYVY_BGRA`, because NDI decodes to
  UYVY natively. SDL picks its desktop `opengl` renderer here, which has no
  native UYVY texture: SDL would convert UYVY→RGB on the CPU (~17 ms per 1080p
  frame). So `I420Frame` repacks UYVY into I420 (~4 ms, buffers reused) and
  SDL's shader does the YUV→RGB conversion and scaling on the GPU. Vertical
  chroma resolution is halved (row pairs averaged) and an odd last row or
  column is dropped. BGRA (only sent by sources with alpha) maps to SDL
  `ARGB8888`. `allow_video_fields = false` makes NDI deinterlace for us.
- **Lifetimes enforce teardown order.** `SourceFinder` and `Receiver` borrow
  `NdiRuntime`; `FrameSync` borrows `Receiver` (NDI requires the frame-sync
  to be destroyed first); `VideoFrame` and `AudioFrame` borrow `FrameSync`
  and free their buffers on drop.
- **NDI FrameSync.** All frames come through NDI's frame synchronizer
  (`NDIlib_framesync_*`), which turns the sender's push into pulls at the
  local clocks: video once per display refresh, audio as fast as the sound
  card plays it. NDI resamples the audio to track the drift between the
  sender's and the sound card's clocks, so audio no longer drifts against
  the buffer (measured: ~190 ppm between a DistroAV sender and the Pi's
  HDMI audio clock, which with the old receive path meant a faded gap
  every ~9 minutes until the buffer had grown). NDI documents concurrent
  calls for receivers, not frame-syncs, so `FrameSync` serializes its NDI
  calls with a lock (microseconds each; frames are not held under it).
  The frame-sync has no error frame, so a lost connection is never
  reported (see Known limitations).
- **Video on the main thread.** `FrameSync::capture_video` returns at once
  with the frame due now (or none before the first arrives), so the loop
  draws once per refresh and vsync paces it. The frame-sync repeats frames
  and reuses buffers, so a new frame is recognised by its `FrameStamp`
  (NDI timestamp and timecode); only a new one is repacked and uploaded,
  otherwise the last picture is presented again. The fps meter and the
  stall detector count new frames only.
- **Audio on its own thread.** `audio_pump` (scoped thread, all the
  threading code) moves audio from the frame-sync into a
  `Mutex<JitterBuffer>` every 5 ms; SDL's audio callback (`Speaker`, on
  SDL's own thread) pulls from it, and the main thread handles video only,
  so display hitches don't starve audio. The pump takes what the buffer
  lacks to its prime level, so the frame-sync is drained exactly as fast
  as the sound card plays (the clock it resamples to). It keeps the first
  2 channels (SDL plays only 1/2/4/6/8) and interleaves them. It stops
  within 5 ms when the video loop ends (also on panic, via a drop guard).
- **Audio device follows the source.** The format is only known once audio
  arrives, and SDL's Rust types are `!Send`, so the main loop calls
  `Speaker::follow_source` every iteration. It opens (or reopens) the device
  in the source's rate and channel count. SDL is asked for exactly that
  format (`allowed_changes = 0`) and converts to the hardware's rate itself.
  A failed open is reported once per format. `VideoWindow::open` takes the
  `Sdl` context, so video and audio share it.
- **Jitter buffer / latency.** Unbroken audio is worth more than closeness
  to the video, so the buffer errs on the side of more audio. Playback
  starts once 100 ms are queued. An underrun fades out, doubles the prime
  level (200, then 400 ms, the limit; "Audio ran dry, now buffering N ms"
  is printed) and re-primes (silence until that much is queued again), so
  a link that stalls is met with more buffering rather than repeated
  gaps. Starts and underruns fade over 5 ms so they don't click. The
  buffer never holds more than the prime level (it is only topped up),
  so there is no backlog trimming. On top of it come the frame-sync's
  own queue (40–63 ms, see below) and SDL's 1024-sample device buffer
  (~21 ms): audio plays ~160 ms or more after arrival. Video goes through
  the frame-sync too, but its delay wasn't measured. No A/V sync by
  timestamps.
- **Working with the frame-sync's audio queue.** Measured on a DistroAV
  source, the frame-sync keeps its own queue at a level it chooses
  (~55–75 ms on Ethernet, up to ~110 ms over Wi-Fi) and fights any other
  level: primed with 300 ms it dropped back to ~70 ms within 5 s, with one
  abrupt ~100 ms skip. When its queue runs (nearly) empty it fills it with
  chunks of exact silence (~67–72 ms; also at start-up), and asked for its
  whole depth it returns only silence. Its own underruns are unfaded hard
  cuts. Filling the jitter buffer from it naively (keeping only 5 ms in
  it) made it run dry and pad over and over: 67 ms gaps every few seconds
  under load. So:
  - The pump leaves 40 ms in the frame-sync (`FRAME_SYNC_RESERVE`; senders
    deliver frames of up to ~33 ms) and never takes more. In a stall the
    frame-sync keeps those 40 ms, doesn't pad, and the jitter buffer runs
    down and handles the underrun as above. On Ethernet the queue went
    below 40 ms in 2 of 10,819 samples.
  - If the queue is below the reserve anyway, the frame-sync has dropped
    or re-timed audio itself (seen every ~90 s on a DistroAV source: 80–
    110 ms dropped at once, a click 1000x the music's high-frequency
    energy). The pump then *splices*: the jitter buffer fades out its end
    and fades in the next audio over 5 ms. Afterwards the click was no
    longer distinguishable from the music.
  - After that, `PaddingRemover` drops exact silence until sound returns,
    so the frame-sync's padding never reaches the buffer as if it were
    audio.
- **No dropout concealment any more.** OBS (DistroAV, 44.1 kHz) was seen
  sending ~14 holes a second of exact 0.0 in every channel (1–5 ms, up to
  ~65 ms while its network struggled), which in music sound like static.
  Earlier versions filled them by cross-fading the sound after a hole into
  the sound before it (the sender inserts the silence, so the waveform
  carries on across it), with a constant 50 ms lookahead. That needs the
  audio as the sender made it. The frame-sync resamples before we see the
  audio, which smears a hole's edges into a band-limited step with ringing
  (only part of the holes keep an exact-zero core). On a synthetic source
  with such holes, the concealer cut the clicky 10 ms windows from 19.4%
  to 0.1% on raw received audio but left them at 18.9% after the
  frame-sync, also with a near-zero threshold and with holes widened by the
  smear (that removes real audio, and the cross-fade then jumps ~1 ms in
  phase). So the concealer (`dropout.rs`, `bridge.rs`) was removed with
  the switch to FrameSync; git history has it. Over wired Ethernet the
  DistroAV source sent no holes at all.
- **Audio frames.** `NDIlib_framesync_capture_audio` with
  `NDIlib_audio_frame_v2_t`, always planar 32-bit float, asked for in the
  source's own rate and channel count (it is asked for those first, with
  zeros); its queue depth is `NDIlib_framesync_audio_queue_depth`.
- **Reconnecting (`--retry-wait`).** NDI reconnects a receiver by itself,
  but a pinned (`--ipaddr`) receiver can't follow a sender that returns on
  another port, and a stale connection can stay silent. `RetryPolicy`
  (from `--retry-wait`) gives up once the `StallDetector` has been stalled
  for the threshold plus the patience; `Viewer::stream_video` then returns
  `Ending::Reconnect` and every NDI object of that connection is dropped
  (the lifetimes force it) before `Viewer::reconnect` creates a new
  finder, as if the program had just started. The `NdiRuntime`, SDL and
  the window are kept, so a kiosk screen never closes. The search after a
  reconnect is polled (`SourceSearch::poll`) once per display refresh
  instead of blocking in `find_source`, so the window keeps handling its
  events. As at start-up, the finder stays alive while the connection
  runs.
- **Kiosk mode.** `WindowMode` decides fullscreen type and cursor
  visibility; `VideoWindow::enter` applies both, at open and on toggle. The
  window is always created resizable so it stays resizable after leaving
  fullscreen. A toggle redraws the last picture at once, because a stalled
  source may not send another frame for a while.
- **Aspect ratio.** `Canvas::set_logical_size` letterboxes the picture inside
  the resizable window. The wait image sets it to its own size while shown
  and hiding it restores the video's. `picture_aspect_ratio` from NDI is
  ignored (square pixels assumed).

## Testing

- `cargo test`: unit tests for argument parsing, source query matching, the
  frame-rate meter, the stall detector, the UYVY→I420 repacking, window mode
  toggling, key handling, audio interleaving and channel narrowing, the
  frame-sync padding remover, the jitter buffer, the discovery scope,
  the retry policy and wait image loading (142 tests; the wait image tests write scratch BMPs
  to the temp directory).
- CI (`.github/workflows/unit-tests.yml`) runs `cargo test` on non-draft PRs
  into `main`. The NDI SDK isn't available there, so it links an empty stub
  `libndi.so`. This works only because unit tests never call NDI functions.
- End-to-end: `tools/build_test_pattern_sender.sh` builds
  `target/test_pattern_sender` (1080p30 UYVY color bars with a moving white
  bar, source name "ndiclient test pattern", plus 48 kHz stereo audio: a
  1 kHz tone at amplitude 0.25 for the first 200 ms of every second). Run
  it, then
  `target/release/ndiclient <this Pi's hostname> "ndiclient test pattern"`.
  Screenshots of the Wayland desktop can be taken with `grim`.
  The audio output can be recorded from the sink's monitor with
  `pw-record --target <sink> -P '{ stream.capture.sink=true }' --format f32 out.wav`
  (sink name: `node.name` in `wpctl inspect <id>`, id from `wpctl status`;
  `pactl` isn't installed). Such recordings had dropouts and didn't line
  up with the received audio on 2026-10-16, with the old build too, so
  for sample-exact comparisons write what the SDL callback receives to a
  file (temporary code) and capture the raw stream with a second,
  audio-only receiver at the same time. Compare builds by running them
  side by side on the same source, since a sender's hiccups vary.

## Known limitations / shortcuts

- When the source disappears, nothing reports an error (the frame-sync has
  no error frame); the window keeps the last frame, "Waiting for video..."
  is printed after 3 s, and NDI reconnects automatically if the source
  returns (or, with `--retry-wait`, ndiclient reconnects from scratch).
- Holes of digital silence that a sender punches into its audio (OBS
  while its network struggles) are not concealed and sound like static;
  see No dropout concealment any more.
- The frame-sync drops or re-times audio by itself now and then (seen
  every ~90 s); each time is spliced with 5 ms fades, a short dip.
  Silence in the source right after such an event, or right after the
  frame-sync ran dry, is dropped like padding, so the buffer underruns
  and raises its level.
- Audio is not synchronised to video by timestamps; it plays ~160 ms or
  more after arrival (frame-sync queue + jitter buffer + device buffer),
  and up to ~480 ms after underruns. Only the first two channels are
  played (no downmix).
- Unsupported pixel formats (e.g. P216/PA16, which we never request) end the
  program with an error.
- Hardcoded: SDK path default, 1280x720 initial window, 10 s discovery
  timeout (also for each search after a reconnect), 5 s listing duration, 5 s fps report interval, 3 s stall threshold, highest bandwidth,
  100/400 ms audio latency, 40 ms left in the frame-sync, 5 ms pump
  interval, 5 ms audio fades, 1024-sample audio device buffer, 2 played
  channels.
- Over Wi-Fi, full-bandwidth 1080p NDI (~90+ Mbit/s) saturates the Pi's
  radio and causes pauses of up to ~12 s. Use wired Ethernet.
- Discovery relies on mDNS (or an NDI discovery server). Where multicast is
  blocked, `--ipaddr` queries the source's host directly. It takes one IP
  address, not a host name or a list. A connection made through it doesn't
  find the source again if the sender restarts on another port; use
  `--retry-wait` (or run ndiclient again).
- Stream name matching is exact and case-sensitive; no partial or wildcard
  matching.
- NDI EULA clause 1.b.iii excludes products running on "embedded devices"
  with Linux. A general-purpose Pi running standard Raspberry Pi OS is likely
  outside that exclusion, but check before distributing.
