# ndiclient - NDI Video Monitor for Linux

An NDI video monitor intended for Raspbian-like distributions.
Helpful when building wireless monitors or (kiosk solutions)[./docs/kiosk].

NDI® is a registered trademark of Vizrt NDI AB. Learn more at [ndi.video](https://ndi.video/).

[![Unit tests](https://github.com/deckerego/ndiclient/actions/workflows/unit-tests.yml/badge.svg)](https://github.com/deckerego/ndiclient/actions/workflows/unit-tests.yml)
[![APT repository](https://github.com/deckerego/ndiclient/actions/workflows/apt-repository.yml/badge.svg)](https://github.com/deckerego/ndiclient/actions/workflows/apt-repository.yml)
[![AI Autonomy Level](https://img.shields.io/badge/AI%20Autonomy-Level%203%20of%205%20(Semi--Autonomous%20Engineer)-blue)](https://blog.jread.com/posts/ai-levels-of-autonomy-in-software-engineering/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](/LICENSE)

## Limitations

*  Right now the target platform is the Raspberry Pi AArch64 platform and tested on an RPi 5 4GB. In theory this arm64 package could work on the RPi 3 or newer (including the RPi Zero 2), but this hasn't been validated yet. Other Linux distros or platforms are currently not available.


## Usage

Run without arguments to list the NDI sources discovered on the network:
```sh
target/release/ndiclient
```

Then pass the NDI device name and stream name of a source. NDI lists sources
as `DEVICE (STREAM)`, so for the source `STUDIO (PGM)`:
```sh
target/release/ndiclient STUDIO PGM
```

The stream name is optional. Without it, the first stream discovered on that
device is shown:
```sh
target/release/ndiclient STUDIO
```

Add `--noaudio` to skip opening an audio device and ignore the audio feed:
```sh
target/release/ndiclient --noaudio STUDIO PGM
```

Add `--fullscreen` to show the video fullscreen instead of in a window:
```sh
target/release/ndiclient --fullscreen STUDIO PGM
```

In fullscreen the mouse cursor is hidden, so the screen shows only the video
(kiosk mode). Press F11 or F to switch between window and fullscreen while
the video plays. Close the window or press Escape to quit.

An empty black screen is easily mistaken for a display that is switched off.
Add `--wait-image` with a BMP file to show that image instead whenever the
source sends no video for 3 seconds. It is scaled to the window, keeping its
aspect ratio, and gives way to the video as soon as the video returns. Other
image formats are rejected at start-up; convert them to BMP first (for
example `ffmpeg -i standby.png standby.bmp`):
```sh
target/release/ndiclient --fullscreen --wait-image standby.bmp STUDIO PGM
```

If mDNS discovery can't be used to find the source, as in a split network
or due to traffic filtering, you can specify the IP address of the source with
`--ipaddr`. Only sources at that address are used and the connection will
remain on that interface. If the connection is interrupted, ndiclient waits 
traffic to resume on the existing open port and interface rather than trying
to negotiate a new one (and possibly switching interfaces to boot). This
should prevent mDNS discovery failure for temporary connection hiccups:
```sh
target/release/ndiclient --ipaddr 192.0.2.20 STUDIO "OBS PGM"
```

Without a device name, `--ipaddr` lists the sources at that address:
```sh
target/release/ndiclient --ipaddr 192.0.2.20
```

When the video stops, ndiclient normally waits for the source to come back
and lets NDI reconnect by itself. Add `--retry-wait` with a number of seconds
to start over instead: once "Waiting for video..." has been shown for n
seconds, the connection is closed and the source is searched for again. 
The window remains open (showing the wait image, if given), 
and a search that finds nothing is repeated until the source is back. 
This option can help `ndiclient` find a source again that may restarted on 
another port, such as when a connection pinned with `--ipaddr`:
```sh
target/release/ndiclient --fullscreen --retry-wait 30 --ipaddr 192.0.2.20 STUDIO PGM
```

The device name is matched ignoring case; the stream name must match exactly.
If no matching source is discovered within 10 seconds, the error lists the
sources that were discovered.


## Installing with APT

Binary packages are available for the Raspberry Pi OS "trixie" and "bookworm"
(arm64) distros.
They are published on an APT source hosted by GitHub Pages for this repo,
so there are a few hoops to jump through in order to install through `apt`.

Each distro has a ready-made APT source file in the
[deb822 format](https://wiki.debian.org/SourcesList) with the repository's
signing key inside it. To add the one for the release you are running and
install `ndiclient`:
```sh
sudo curl -fsSLo /etc/apt/sources.list.d/ndiclient.sources \
    "https://deckerego.github.io/ndiclient/$(. /etc/os-release && echo "$VERSION_CODENAME").sources"
sudo apt update
sudo apt install ndiclient
```

Later releases then arrive with `sudo apt upgrade`.


## Performance Hacks

* Turning off wifi power saving may help a bit with packet loss. Run `sudo iw dev wlan0 set power_save off` to try it out.
* Disable swap to avoid wear & tear on your SD card by editing `/etc/rpi/swap.conf` and setting `FixedSizeMiB=0`.
* Use wired connections whenever possible.

## Building & Packaging

See [BUILD.md](./BUILD.md) for instructions on how to build & package ndiclient.


## License

This code is made available with the [MIT License](./LICENSE).

This code uses the NDI SDK, whose terms are available at:
[https://docs.ndi.video/all/developing-with-ndi/sdk/licensing](https://docs.ndi.video/all/developing-with-ndi/sdk/licensing)
