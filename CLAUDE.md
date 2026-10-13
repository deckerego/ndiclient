# ndiclient

Proof-of-concept command line NDI receiver for Raspberry Pi OS (Raspbian) ARM64.
`ndiclient <ip-address> [source-name]` finds an NDI source running on that host
(optionally by name) and shows its video in a desktop window. Video only; audio
is deliberately out of scope for now.

## Project constraints

- Proof of concept: shortcuts and hardcoded values are fine when they keep the
  code smaller and simpler.
- Rust is preferred; C or C++ only if it makes the code much simpler.
- Audio comes later, if the project proves feasible.
- Failure is an acceptable outcome. If the Pi can't reasonably do this, stop
  and write up the limitations and lessons learned instead of forcing it.
- Inspiration/prior art: Dicaffeine (https://dicaffeine.com/), which uses a
  libyuri fork (https://github.com/melnijir/libyuri) but is unmaintained.

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
target/release/ndiclient 192.168.1.20
target/release/ndiclient 192.168.1.20 "OBS PGM"
target/release/ndiclient 192.168.1.20 "STUDIO.LOCAL (OBS PGM)"
```

- Close the window or press Escape to quit.
- Prints the displayed frame rate every 5 seconds.
- The source name may be the short name or the full NDI name, matched exactly
  (case-sensitive). Without a name, the first source discovered on the host is used.
- Gives up with exit code 1 if no matching source is found within 10 s; the
  error lists the source names seen on that host.
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
- The `sdl2` crate is used with the `unsafe_textures` feature so a texture can
  live in the same struct as its `TextureCreator` (no self-referential lifetimes).
- NDI bindings are hand-written (`src/ndi/ffi.rs`), not bindgen, to avoid a
  libclang dependency. They cover only the functions used. Keep them in sync
  with the SDK headers in `include/` if the SDK is upgraded.

## Architecture

```
src/main.rs          argument parsing, wiring, main capture/display loop
src/picture.rs       Picture / PixelLayout / Dimensions: display-agnostic frame view
src/display.rs       VideoWindow: SDL2 window, streaming texture, letterboxing
src/frame_rate.rs    FrameRateMeter: periodic fps reporting
src/stall.rs         StallDetector: "waiting for video" once per stall
src/ndi/ffi.rs       raw C declarations for the NDI SDK
src/ndi/runtime.rs   NdiRuntime: RAII for NDIlib_initialize/NDIlib_destroy
src/ndi/query.rs     SourceQuery: host + optional name, matching rules
src/ndi/finder.rs    SourceFinder / SourceDescriptor: find the source matching a query
src/ndi/receiver.rs  Receiver / VideoFrame / Capture: receive video frames
tools/               test-pattern NDI sender used for development (C)
```

Key decisions:

- **Connecting by IP.** NDI connects by source name, not by IP. The finder is
  created with the IP in `p_extra_ips`, which queries that host directly (works
  without multicast/mDNS). We then pick the first source whose `url_address`
  (`ip:port`) is on that host and, if a name was given, whose full name
  (`MACHINE (Source)`) or short name (`Source`) equals it. NDI may list a
  source twice (mDNS + direct query); names are de-duplicated in errors.
- **Black window on open.** Wayland maps a window only after its first
  present, so `VideoWindow::open` presents a cleared (black) frame.
- **Pixel format.** The receiver asks for `UYVY_BGRA`. NDI decodes to UYVY
  natively, and SDL uploads UYVY textures directly, so the GPU does the YUV→RGB
  conversion and scaling. BGRA (only sent by sources with alpha) maps to SDL
  `ARGB8888`. `allow_video_fields = false` makes NDI deinterlace for us.
- **Lifetimes enforce teardown order.** `SourceFinder` and `Receiver` borrow
  `NdiRuntime`; `VideoFrame` borrows `Receiver` and frees its buffer on drop.
- **Single thread.** Capture uses a 100 ms timeout so the SDL event loop stays
  responsive. Vsync paces presentation. No threading code exists yet.
- **Aspect ratio.** `Canvas::set_logical_size` letterboxes the picture inside
  the resizable window. `picture_aspect_ratio` from NDI is ignored (square
  pixels assumed).

## Testing

- `cargo test`: unit tests for source query matching, the frame-rate meter
  and the stall detector (16 tests).
- CI (`.github/workflows/unit-tests.yml`) runs `cargo test` on non-draft PRs
  into `main`. The NDI SDK isn't available there, so it links an empty stub
  `libndi.so`. This works only because unit tests never call NDI functions.
- End-to-end: `tools/build_test_pattern_sender.sh` builds
  `target/test_pattern_sender` (1080p30 UYVY color bars with a moving white
  bar, source name "ndiclient test pattern"). Run it, then
  `target/release/ndiclient <this Pi's IP>`. Screenshots of the Wayland
  desktop can be taken with `grim`.

## Results so far (2026-10-13)

- Works end to end on the Pi 5 desktop. The 1080p30 test pattern, sent from the
  same Pi, displays at a steady 30.0 fps with correct colors and motion.
- Cost: ndiclient used about 68% of one core (~125 MB RSS); the sender used
  about 40%. Decoding is NDI's software SpeedHQ decoder on the CPU; display is
  GPU-composited via SDL.
- Remote source "RONINEGO.LOCAL (Test Patterns)" at 192.168.129.208:5961, over
  this Pi's Wi-Fi (2026-10-13): **works.** A 1920x1080 UYVY chroma multiburst
  pattern displays correctly with no code changes.
  - First attempt received no video at all (the SDK's `ndi-record` didn't get
    any either). The cause was on the sender side: its firewall was on.
    After the user turned it off, video flowed.
  - For a static pattern this sender delivered only 1 frame per 10 s (NDI
    receive stats: 1 total, 0 dropped), although it advertises 29.97 fps.
    So the fps lines are not printed (the meter only reports when a frame
    arrives after the 5 s interval) and remote throughput is still unmeasured.
    Use a moving source to measure it.
- Remote live source "RONINEGO.LOCAL (OBS PGM)", 1920x1080 UYVY at 60/1
  fps progressive, over Wi-Fi (5 GHz, 433 Mbit/s link, -60 dBm), 2026-10-13:
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
  against RONINEGO by short name, full name and a wrong name (error lists
  the host's sources). Also verified against the local sender: stopping it
  prints "Waiting for video..." after 3 s, NDI reconnects automatically when
  it restarts, a second stop prints the message again, and the black window
  shows while no video has arrived.
- Not yet tested: wired Ethernet (to confirm a full 60 fps), choosing between
  two sources that are live on one host at the same time, NDI|HX
  (H.264/H.265) sources, sources with alpha, interlaced sources.

## Known limitations / shortcuts

- When the source disappears, NDI does not report an error; the window keeps
  the last frame, "Waiting for video..." is printed after 3 s, and NDI
  reconnects automatically if the source returns.
  `Capture::ConnectionLost` only handles NDI's explicit error frame type.
- Unsupported pixel formats (e.g. P216/PA16, which we never request) end the
  program with an error.
- Hardcoded: SDK path default, 1280x720 initial window, 10 s discovery
  timeout, 5 s fps report interval, 3 s stall threshold, highest bandwidth.
- Name matching is exact and case-sensitive; no partial or wildcard matching.
- NDI EULA clause 1.b.iii excludes products running on "embedded devices"
  with Linux. A general-purpose Pi running standard Raspberry Pi OS is likely
  outside that exclusion, but check before distributing.

## Possible next steps

- Audio (NDI `capture_v3` audio frames → SDL audio queue), likely on its own
  thread or via NDI FrameSync.
- Fullscreen toggle / kiosk mode.
- Measure with real network sources at 1080p60 and with NDI|HX.
