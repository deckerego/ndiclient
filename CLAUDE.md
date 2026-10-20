# ndiclient

Command line NDI receiver for Raspberry Pi OS (Raspbian) ARM64.
`ndiclient [-d] [--fullscreen] [--noaudio] [--ipaddr <ip>] [--wait-image <file.bmp>] <device-name> [stream-name]`
uses NDI discovery to find the source named `DEVICE (Stream)` (or the first
stream on that device), shows its video in a desktop window or fullscreen
(or an optional image while no video arrives) and plays its audio.
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
  for listing. For multicast lost between Wi-Fi and wired Ethernet, and to
  pick one interface of a multi-homed source. Without a device name it
  lists the sources at that address. Options may come in any order, each
  at most once; `--ipaddr` and `--wait-image` take the next argument as
  their value.
- `--wait-image <file.bmp>` shows that image whenever "Waiting for
  video..." is printed (also at start-up, after 3 s without a first
  frame), letterboxed to the window like the video, until the next new
  frame. BMP only (`SDL_LoadBMP`, so no SDL_image dependency); the file is
  read before discovery, and a missing or non-BMP file exits 1 at once.
  The image's texture exists only while it is shown. Not valid without a
  device name.
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
                     -d, --fullscreen, --noaudio, --ipaddr, --wait-image),
                     wiring, source listing, main capture/display loop
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
src/stall.rs         StallDetector: "waiting for video" once per stall
src/wait_image.rs    WaitImage: the BMP shown while no video arrives
src/ndi/ffi.rs       raw C declarations for the NDI SDK
src/ndi/runtime.rs   NdiRuntime: RAII for NDIlib_initialize/NDIlib_destroy
src/ndi/query.rs     SourceQuery: device + optional stream name, matching rules
src/ndi/finder.rs    SourceFinder / SourceDescriptor: list sources, find one matching a query
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
  receiver connects to the chosen descriptor's URL, so it uses that
  interface too. Earlier PoC versions required an IP; it became optional
  when discovery by name was added.
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
  frame-sync padding remover, the jitter buffer, the discovery scope and
  wait image loading (123 tests; the wait image tests write scratch BMPs
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

## Results so far (2026-10-13)

- Works end to end on the Pi 5 desktop. The 1080p30 test pattern, sent from the
  same Pi, displays at a steady 30.0 fps with correct colors and motion.
- Cost: ndiclient used about 68% of one core (~125 MB RSS); the sender used
  about 40%. Decoding is NDI's software SpeedHQ decoder on the CPU; display is
  GPU-composited via SDL.
- Remote source over this Pi's Wi-Fi (2026-10-13): **works.** A 1920x1080 
  UYVY chroma multiburst pattern displays correctly with no code changes.
  - First attempt received no video at all (the SDK's `ndi-record` didn't get
    any either). The cause was on the sender side: its firewall was on.
    After the user turned it off, video flowed.
  - For a static pattern this sender delivered only 1 frame per 10 s (NDI
    receive stats: 1 total, 0 dropped), although it advertises 29.97 fps.
    So the fps lines are not printed (the meter only reports when a frame
    arrives after the 5 s interval) and remote throughput is still unmeasured.
    Use a moving source to measure it.
- Remote live source of 1920x1080 UYVY at 60/1 fps progressive, 
  over Wi-Fi (5 GHz, 433 Mbit/s link, -60 dBm), 2026-10-13:
  **works; average 42.3 fps displayed** (5 s reports ranged 23.5–50.5) over
  45 s, at about 138% of one core (~1/3 of the Pi 5's 4 cores), ~185 MB RSS,
  SoC at 71 °C.
  - Baseline without display (C program: NDI receive + decode only, `/tmp`
    diagnostic, not in repo): 44.4 fps over 45 s, NDI stats 2002 frames
    received / 5 dropped, Wi-Fi carrying ~103 Mbit/s.
  - Conclusion: the 60 → ~44 fps shortfall is upstream of the Pi (Wi-Fi
    throughput or the sender's actual output rate). NDI's decoder keeps up
    with what arrives, and SDL display costs only ~2 fps. To separate the
    network from the sender, test over wired Ethernet next.
- Source selection by name and the waiting message (2026-10-13): verified
  against remote source by short name, full name and a wrong name (error lists
  the host's sources). Also verified against the local sender: stopping it
  prints "Waiting for video..." after 3 s, NDI reconnects automatically when
  it restarts, a second stop prints the message again, and the black window
  shows while no video has arrived.
- Discovery by device + stream name (2026-10-13): over mDNS, the local test
  pattern was found as `<HOSTNAME> (ndiclient test pattern)` with the device
  given in lowercase and displayed at ~30 fps; the remote source 
  was found and connected. A wrong stream name lists all discovered sources.
- Listing (2026-10-13): `ndiclient` with no arguments printed both sources
  above after 5 s; with the local sender stopped, only the remote one.
- Optional stream name (2026-10-13): `ndiclient <hostname>` connected to the local
  test pattern (~31 fps) and `ndiclient <DEVICE>` to the remote source.
- Fullscreen (2026-10-13): `--fullscreen` with the local test pattern filled
  the 1920x1080 HDMI display (screenshot via `grim`) at ~30 fps. The mouse
  cursor stayed visible (fixed by kiosk mode, below). Escape was not tested
  by injected keypress (no `wtype`/`ydotool` installed); it uses the same
  event handling as windowed.
- Profiling and the I420 upload (2026-10-13): the 1080p30 remote source over Wi-Fi.
  - CPU per frame before the change: `texture.update` ~17 ms (SDL's CPU
    UYVY→RGB), capture/present negligible. After: repack + upload ~4.6 ms.
  - Same source, back-to-back 20 s samples: 81–82% → 47–48% of one core,
    RSS ~123 → ~105 MB, fps unchanged (30). Local test pattern: 64% → 27%,
    116 → 95 MB. Colour bars verified by screenshot, also in fullscreen.
  - Pauses after 30–60 s were reproduced: freezes of 4.3 s and 12.1 s plus
    several 0.3–1 s hitches, and ~100–150 ms late frames about twice a
    second (hidden by the 5 s fps average). In every pause NDI's own
    counters showed 0 frames received, 0 dropped, empty queue, so the client
    was idle, not stuck. A concurrent ping to the sender lost ~10% with
    100–300 ms RTT, and the loss lined up with the video pauses; without the
    stream the same ping is clean (0% loss, 17 ms). The stream averages
    92 Mbit/s (peaks 137) on the Pi's BCM43455 Wi-Fi (433 Mbit/s link,
    -55 dBm, power save on), close to what it sustains. Cause: Wi-Fi
    congestion, not the client. Fix with wired Ethernet or a lower-bitrate
    source (NDI|HX).
- Kiosk mode (2026-10-14): `--fullscreen` with the local test pattern at
  ~31 fps; a `grim -c` screenshot (which includes the cursor, checked against
  a desktop screenshot) shows no cursor over the picture. Windowed mode still
  shows it. The F11/F toggle was not tested by injected keypress (no
  `wtype`/`ydotool`); its key mapping is covered by unit tests.
- Debian package (2026-10-14): `packaging/build_deb.sh` builds
  `ndiclient_0.1.0-1_arm64.deb` (~1.4 MB). Contents, permissions (root-owned),
  RUNPATH `/usr/lib/ndiclient` and the generated `Depends` were checked. The
  extracted binary ran discovery with the bundled libndi (no sender was live,
  so none were listed). Not yet installed with `apt`/`dpkg -i` (needs sudo)
  and not checked with `lintian` (not installed). The changelog and the man
  page (renders cleanly with `man -l`) were added afterwards.
- Audio (2026-10-14): local test pattern over the HDMI sink (PipeWire 1.4).
  A 10 s recording of the sink monitor had all 10 beeps, each exactly
  200 ms long and 1000 ms apart, with no dropouts, at ~1 kHz and amplitude
  0.250 (unchanged). CPU ~30.5% with audio vs ~28.8% with `--noaudio`
  (~106 MB RSS both). `--noaudio` showed no PipeWire stream and 4 fewer
  threads. `SDL_AUDIODRIVER=nonexistent` printed the warning and video
  continued. SIGTERM exits cleanly (exit 0, the audio thread joins). Not
  tested: remote sources with audio, A/V sync by eye/ear, long runs (drift),
  more than 2 channels, rates other than 48 kHz.
- Audio static from OBS (2026-10-14): Remote DistroAV instance on OBS over
  Wi-Fi, 44.1 kHz stereo; music had static, voice sounded fine. Delivery
  was mostly steady (0 simulated underruns in 4 of 5 30 s runs; one Wi-Fi
  burst gave a 715 ms gap and 7). The static was holes of digital silence
  already in the received audio (see No dropout concealment any more). Recorded at the
  HDMI sink monitor, 2 x 21 s each: ~9 holes/s before, ~0.1/s after.
- Audio fill and buffering (2026-10-14): Remote DistroAV instance on OBS over
  Wi-Fi, 60 s recorded at the HDMI sink monitor, with the concealer's
  input and output dumped for a like-for-like comparison. Input: 14.2
  holes/s, 17 longer than 10 ms, 3 longer than 25 ms. At the holes the
  fill had 6% clicky and 3% dull spots (straight line on the same input:
  20% / 41%); the recording had 0.1 holes/s and ~1.7 envelope dips/s
  (~15/s with the previous build on the same stream). Two underruns in
  one Wi-Fi stall at ~51 s (143 ms, then 1.5 s of silence) raised the
  buffering to 400 ms and none followed; the sender's 27 and 41 ms holes
  passed unchanged. ~60% of one core, ~110 MB RSS, 30 fps unchanged.
  Delivery was clean in NDI's own counters throughout (0 dropped), so the
  remaining gaps are upstream stalls, not the client. Listening test
  still to do.
- `--ipaddr` (2026-10-14): listing at this Pi's Wi-Fi address showed the
  local test pattern, at an unused address none (exit 0); connecting to
  the test pattern through it ran at ~31 fps, and at the unused address it
  failed after 10 s saying nothing was discovered there. Remote DistroAV
  instance on OBS at its address was listed and played (30 fps, 44.1 kHz
  audio). Not tested where it matters: a network where mDNS doesn't reach
  the source, and a source with two interfaces.
- NDI FrameSync (2026-10-16): DistroAV instance on OBS, 1080p29.97 with
  44.1 kHz stereo, over wired Ethernet (gigabit switch) and over Wi-Fi;
  each build's audio as handed to SDL compared with a raw audio-only
  receiver, and against the previous build running at the same time.
  - Ethernet, 3 and 5 min side by side: no gaps in either build. The
    output followed the raw audio with a lag changing at ~190 ppm (NDI
    correcting the clock difference) instead of bit-exact at a constant
    offset. The frame-sync re-timed its audio three times in 5 min
    (80–110 ms dropped at once); with the splice fades they didn't stand
    out from the music. 30 fps displayed.
  - Wi-Fi, 90 s: no gaps. 120 s: one 16 s stall of the link (the raw
    receiver also got only 104 s of audio), met with the underrun fades
    and the buffer raised to 400 ms; no padding gaps. Before the 40 ms
    reserve, padding gaps of 18–170 ms came through.
  - Start-up underrun ("Audio ran dry, now buffering 200 ms" within the
    first second): 1 in 5 starts, previous build 2 in 5.
  - CPU, both builds side by side on the same source: 56% vs 53% of one
    core, ~100 vs ~96 MB RSS; the frame-sync's thread ~1%, the pump ~0.4%.
  - Local test pattern: colour bars correct, 30 fps; 12 beeps of 200 ms,
    1000 ms apart, 1 kHz, amplitude 0.25. Stopping the sender printed
    "Waiting for video..." and an underrun, restarting it resumed 30 fps;
    SIGTERM exit 0.
- Wait image (2026-10-19): local test pattern with a 640x360 BMP,
  windowed: stopping the sender showed the image after 3 s (screenshot),
  restarting it brought the colour bars back, a second stop showed the
  image again. Fullscreen with a 400x300 BMP: scaled up and pillarboxed.
  A non-BMP file and a missing file each exit 1 before discovery, and
  `--wait-image` without a device name prints the usage.
- Not yet tested: a 60 fps source over wired Ethernet, choosing between
  two sources that are live on one host at the same time, NDI|HX
  (H.264/H.265) sources, sources with alpha, interlaced sources.

## Known limitations / shortcuts

- When the source disappears, nothing reports an error (the frame-sync has
  no error frame); the window keeps the last frame, "Waiting for video..."
  is printed after 3 s, and NDI reconnects automatically if the source
  returns.
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
  timeout, 5 s listing duration, 5 s fps report interval, 3 s stall threshold, highest bandwidth,
  100/400 ms audio latency, 40 ms left in the frame-sync, 5 ms pump
  interval, 5 ms audio fades, 1024-sample audio device buffer, 2 played
  channels.
- Over Wi-Fi, full-bandwidth 1080p NDI (~90+ Mbit/s) saturates the Pi's
  radio and causes pauses of up to ~12 s. Use wired Ethernet.
- Discovery relies on mDNS (or an NDI discovery server). Where multicast is
  blocked, `--ipaddr` queries the source's host directly. It takes one IP
  address, not a host name or a list.
- Stream name matching is exact and case-sensitive; no partial or wildcard
  matching.
- NDI EULA clause 1.b.iii excludes products running on "embedded devices"
  with Linux. A general-purpose Pi running standard Raspberry Pi OS is likely
  outside that exclusion, but check before distributing.

## Possible next steps

- A/V sync: delay the video to match the audio (both now come through the
  frame-sync).
- Conceal the sender's holes again despite the frame-sync's resampling,
  e.g. with a pitch-matched fill across the smeared hole.
