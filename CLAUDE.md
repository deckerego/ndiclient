# ndiclient

Command line NDI receiver for Raspberry Pi OS (Raspbian) ARM64.
`ndiclient [--fullscreen] [--noaudio] <device-name> [stream-name]` uses NDI
discovery to find the source named `DEVICE (Stream)` (or the first stream on
that device), shows its video in a desktop window or fullscreen and plays its
audio. `ndiclient` without arguments lists the sources it discovers.

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

## Usage

```sh
cargo build --release
target/release/ndiclient
target/release/ndiclient STUDIO "OBS PGM"
target/release/ndiclient studio.local "OBS PGM"
target/release/ndiclient STUDIO
target/release/ndiclient --fullscreen STUDIO "OBS PGM"
target/release/ndiclient --noaudio STUDIO
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
  doesn't capture audio; it isn't valid without a device name either. Options
  may come in any order, each at most once. "Playing audio: 48000 Hz, 2
  channels" is printed when the output opens (again if the format changes).
  If no output can be opened, a warning is printed and the video still plays.
- Kiosk mode: in fullscreen the mouse cursor is hidden. F11 or F switches
  between windowed and fullscreen at runtime (held keys don't repeat it).
- Close the window or press Escape to quit (also in fullscreen).
- Prints the displayed frame rate every 5 seconds.
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
  `packaging/debian/changelog`; the build fails if its upstream part differs
  from Cargo.toml's version, so bump both together (and add a changelog
  entry). Keep `packaging/debian/ndiclient.1` in sync with the usage.
- Distribution: `.github/workflows/apt-repository.yml` builds the package in
  a `debian:trixie` container on GitHub's `ubuntu-24.04-arm` runner. It
  downloads the NDI SDK from NDI's public URL and runs its installer with
  `y` piped in (the repository owner accepted the EULA), after checking the
  download against `NDI_SDK_SHA256`. NDI replaces that download in place,
  so a new SDK release fails the check until the checksum is updated. It
  runs only when a GitHub release is published (its tag `vX.Y.Z` must match
  Cargo.toml). Releases created with the workflow `GITHUB_TOKEN` don't
  trigger it (GitHub prevents that). Two jobs keep the signing key away
  from third-party code: `build` (NDI installer, cargo; read-only token, no
  secrets) uploads the `.deb` as an artifact; `publish` downloads it in a
  fresh container, runs `packaging/update_apt_repo.sh` on the `gh-pages`
  branch and pushes it; GitHub Pages serves it at
  https://deckerego.github.io/ndiclient (distribution `trixie`, component
  `main`). Actions are pinned to commit SHAs and `debian:trixie` to a
  digest; update the pins deliberately. `publish` uses the GitHub
  environment `publishing` (deployment rule: tags `*.*.*` only), which
  holds the secret `SIGNING_PRIVATE_KEY` (must have no passphrase) and the
  variable `SIGNING_PUBLIC_KEY` the signature is checked against. Keep
  them out of repository-level secrets and variables. `update_apt_repo.sh` keeps old packages in
  `pool/` and refuses to replace a `.deb` with different content, so a
  rebuild of one version needs a new changelog entry. It needs `apt-utils`
  and `gnupg`, and the key id in `APT_SIGNING_KEY`.
- The `sdl2` crate is used with the `unsafe_textures` feature so a texture can
  live in the same struct as its `TextureCreator` (no self-referential lifetimes).
- NDI bindings are hand-written (`src/ndi/ffi.rs`), not bindgen, to avoid a
  libclang dependency. They cover only the functions used. Keep them in sync
  with the SDK headers in `include/` if the SDK is upgraded.

## Architecture

```
src/main.rs          argument parsing (Command, ViewOptions: --fullscreen,
                     --noaudio), wiring, source listing, main
                     capture/display loop
src/picture.rs       Picture / PixelLayout / Dimensions: display-agnostic frame view
src/sound.rs         Sound / AudioFormat: device-agnostic planar audio view,
                     interleaving, channel narrowing
src/dropout.rs       DropoutConcealer: finds short runs of digital silence
                     that some senders punch into the audio and fills them
                     (constant lookahead)
src/bridge.rs        fills a hole in continuous audio by cross-fading the
                     sound after it into the sound before it
src/jitter_buffer.rs JitterBuffer / AudioLatency: audio queue between the
                     network and the sound card (adaptive priming, faded
                     starts/stops/cuts, backlog trimming)
src/audio_pump.rs    the only threading code: audio capture thread
src/speaker.rs       Speaker: SDL audio output fed from the jitter buffer
src/i420.rs          I420Frame: repacks UYVY into planar I420 for the GPU
src/display.rs       VideoWindow / WindowMode: SDL2 window or fullscreen,
                     key handling, cursor hiding, streaming texture,
                     letterboxing
src/frame_rate.rs    FrameRateMeter: periodic fps reporting
src/stall.rs         StallDetector: "waiting for video" once per stall
src/ndi/ffi.rs       raw C declarations for the NDI SDK
src/ndi/runtime.rs   NdiRuntime: RAII for NDIlib_initialize/NDIlib_destroy
src/ndi/query.rs     SourceQuery: device + optional stream name, matching rules
src/ndi/finder.rs    SourceFinder / SourceDescriptor: list sources, find one matching a query
src/ndi/receiver.rs  Receiver / VideoFrame / AudioFrame / Capture: receive frames
tools/               test-pattern NDI sender used for development (C)
packaging/           build_deb.sh, update_apt_repo.sh; Debian control
                     template, copyright, changelog and man page
                     (packaging/debian/)
```

Key decisions:

- **Discovery by name.** The finder uses NDI's default discovery (mDNS, or a
  discovery server if configured in NDI's own config file); no `p_extra_ips`.
  We pick the first source whose full name (`DEVICE (Stream)`) splits into the
  requested device and (if given) stream. "First" is NDI's list order, which
  is roughly discovery order, so with several streams on one device and no
  stream name the choice is not guaranteed to be stable between runs. NDI may list a source more than once; names are
  de-duplicated in errors. Earlier PoC versions connected by IP via
  `p_extra_ips`, which also worked without multicast; that was removed.
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
  `NdiRuntime`; `VideoFrame` and `AudioFrame` borrow `Receiver` and free
  their buffers on drop.
- **Video on the main thread.** Video capture uses a 100 ms timeout so the SDL
  event loop stays responsive. Vsync paces presentation.
- **Audio on its own thread.** NDI allows capture on several threads at once,
  so `audio_pump` (scoped thread, all the threading code) captures audio
  only, and the main thread video only. Display hitches therefore don't
  starve audio. The pump keeps the first 2 channels (SDL plays only 1/2/4/6/8),
  interleaves them, conceals dropouts (below) and pushes them into a
  `Mutex<JitterBuffer>`. SDL's audio
  callback (`Speaker`, on SDL's own thread) pulls from it. The pump stops
  within 100 ms when the video loop ends (also on panic, via a drop guard).
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
  gaps. A backlog beyond the prime level plus 150 ms (bursts, or a sender
  clock faster than the sound card's) is cut back to the prime level with
  a 5 ms cross-fade; starts and underruns fade over 5 ms too, so none of
  them click. There is no resampling for clock drift: the buffer absorbs
  it (at 50 ppm, ~30 min until a re-prime when the sender is slow, ~50 min
  until a cut when it is fast). Audio also has SDL's 1024-sample device
  buffer (~21 ms) and the concealer's 50 ms lookahead, so it plays
  ~170 ms or more after arrival, while video is shown on arrival. No A/V
  sync by timestamps.
- **Dropout concealment.** OBS (DistroAV, 44.1 kHz) was seen sending
  ~14 holes a second: 1–5 ms (up to ~65 ms while its network struggled)
  of exact 0.0 in every channel, mid-frame. They come from the sender,
  not the network (same rate with `NDIlib_recv_bandwidth_audio_only`),
  and in music they sound like static. The sender *inserts* the silence
  rather than replacing sound with it: the waveform after a hole carries
  straight on from the one before it (a forward prediction from before a
  hole matches the audio after it 7x better that way than if a hole's
  worth of audio were missing). So `DropoutConcealer` fills any
  all-channel run of exact silence of up to 25 ms that follows sound with
  a cross-fade from the sound after the hole into the sound before it
  (`bridge`): both seams are continuous and the music keeps its tone.
  Longer silence passes unchanged. All audio is held back a constant
  50 ms (twice the limit) so the hole and the sound after it have
  arrived; being constant, the delay adds no jitter. On 65 s of captured
  music, the high-frequency energy at the holes relative to their
  surroundings was 82x for the raw holes (91% clicky), 0.53x (20% clicky,
  41% dull spots) for the straight line used before and 0.76x (6% clicky,
  3% dull) for this fill. Two-sided LPC extrapolation was tried first and
  was no better than the straight line, because it assumes the audio
  under the hole is missing.
- **Audio frames.** `recv_capture_v2` with `NDIlib_audio_frame_v2_t`, which
  is always planar 32-bit float (no FourCC to check).
- **Kiosk mode.** `WindowMode` decides fullscreen type and cursor
  visibility; `VideoWindow::enter` applies both, at open and on toggle. The
  window is always created resizable so it stays resizable after leaving
  fullscreen. A toggle redraws the last picture at once, because a stalled
  source may not send another frame for a while.
- **Aspect ratio.** `Canvas::set_logical_size` letterboxes the picture inside
  the resizable window. `picture_aspect_ratio` from NDI is ignored (square
  pixels assumed).

## Testing

- `cargo test`: unit tests for argument parsing, source query matching, the
  frame-rate meter, the stall detector, the UYVY→I420 repacking, window mode
  toggling, key handling, audio interleaving, dropout concealment, the
  bridge fill and the jitter buffer (90 tests).
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
  (sink name from `pactl get-default-sink`).

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
  pattern was found as `NDIDEV (ndiclient test pattern)` with the device given
  in lowercase (`ndidev`) and displayed at ~30 fps; the remote source 
  was found and connected. A wrong stream name lists all discovered sources.
- Listing (2026-10-13): `ndiclient` with no arguments printed both sources
  above after 5 s; with the local sender stopped, only the remote one.
- Optional stream name (2026-10-13): `ndiclient ndidev` connected to the local
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
- Audio static from OBS (2026-10-14): DistroAV OBS source over
  Wi-Fi, 44.1 kHz stereo; music had static, voice sounded fine. Delivery
  was mostly steady (0 simulated underruns in 4 of 5 30 s runs; one Wi-Fi
  burst gave a 715 ms gap and 7). The static was holes of digital silence
  already in the received audio (see Dropout concealment). Recorded at the
  HDMI sink monitor, 2 x 21 s each: ~9 holes/s before, ~0.1/s after.
- Audio fill and buffering (2026-10-14): DistroAV OBS source over
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
- Not yet tested: wired Ethernet (to confirm a full 60 fps), choosing between
  two sources that are live on one host at the same time, NDI|HX
  (H.264/H.265) sources, sources with alpha, interlaced sources.

## Known limitations / shortcuts

- When the source disappears, NDI does not report an error; the window keeps
  the last frame, "Waiting for video..." is printed after 3 s, and NDI
  reconnects automatically if the source returns.
  `Capture::ConnectionLost` only handles NDI's explicit error frame type.
- Dropout concealment is heuristic: genuine digital silence of 25 ms or
  less after sound is filled too (it gets a short cross-fade of the
  surrounding sound), and holes longer than 25 ms (`MAX_DROPOUT` in
  `audio_pump.rs`, the knob to turn if a sender punches longer ones) or
  not exactly 0.0 are not concealed. The fill assumes the sender inserted
  the silence; a sender that drops audio instead gets a fill that repeats
  a hole's worth of sound.
- Audio is not synchronised to video by timestamps; it plays ~170 ms or
  more after arrival (concealer lookahead + jitter buffer + device
  buffer), and up to ~470 ms after underruns. Clock drift between sender
  and sound card is absorbed by the jitter buffer, with a faded cut or
  gap every few tens of minutes instead of resampling. Only the first two
  channels are played (no downmix).
- Unsupported pixel formats (e.g. P216/PA16, which we never request) end the
  program with an error.
- Hardcoded: SDK path default, 1280x720 initial window, 10 s discovery
  timeout, 5 s listing duration, 5 s fps report interval, 3 s stall threshold, highest bandwidth,
  100/400 ms (+150 ms headroom) audio latency, 5 ms audio fades, 25 ms
  max concealed dropout, 1024-sample audio device buffer, 2 played
  channels.
- Over Wi-Fi, full-bandwidth 1080p NDI (~90+ Mbit/s) saturates the Pi's
  radio and causes pauses of up to ~12 s. Use wired Ethernet.
- Discovery relies on mDNS (or an NDI discovery server). Networks that block
  multicast need a discovery server; there is no IP fallback any more.
- Stream name matching is exact and case-sensitive; no partial or wildcard
  matching.
- NDI EULA clause 1.b.iii excludes products running on "embedded devices"
  with Linux. A general-purpose Pi running standard Raspberry Pi OS is likely
  outside that exclusion, but check before distributing.

## Possible next steps

- A/V sync by timestamps, or NDI FrameSync for drift-free audio (it would
  also replace the video capture path).
- Measure with real network sources at 1080p60 and with NDI|HX.
