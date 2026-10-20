# ndiclient - NDI Video Monitor for Linux

An NDI video monitor intended for Raspbian-like distributions.
I couldn't find another CLI that allowed me to stream NDI sources,
so this is my attempt at bootstrapping one.

NDI® is a registered trademark of Vizrt NDI AB. Learn more at [ndi.video](https://ndi.video/).


## Limitations

*  Right now the receiver requests the highest bandwidth feed available, low bandwidth mode is not supported.
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

If mDNS discovery doesn't find the source, for example because multicast gets
lost between Wi-Fi and wired Ethernet, give the IP address of the source with
`--ipaddr`. That address is then asked for its sources directly, and only
sources at that address are used, so it also picks the network interface of a
source that has several:
```sh
target/release/ndiclient --ipaddr 192.0.2.20 STUDIO "OBS PGM"
```

Without a device name, `--ipaddr` lists the sources at that address:
```sh
target/release/ndiclient --ipaddr 192.0.2.20
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

Turning off wifi power saving may help a bit with packet loss. 
Run `sudo iw dev wlan0 set power_save off` to try it out.


## Building & Packaging

You can also build from source if you are keen to...

### Building from Source

Currently `ndiclient` requires `libsdl2-dev` and the NDI SDK available at
[https://ndi.video/for-developers/ndi-sdk/](https://ndi.video/for-developers/ndi-sdk/).
The environment var `NDI_SDK_DIR` can be set to the path of the NDI SDK,
it defaults to `/home/pi/NDI SDK for Linux` otherwise.

After those are installed the build should obey Cargo:
```sh
cargo build --release
```

### Testing

A test pattern sender is available in the `tools/` directory for local integration testing.

Unit testing can be performed using:
```sh
cargo test
```

### Creating Debian Packages

To build a `.deb` for the local machine's architecture, run:
```sh
packaging/build_deb.sh
```

This needs `dpkg-dev` (for `dpkg-shlibdeps`) in addition to the build
requirements above. The package lands in `target/debian/` and installs with:
```sh
sudo apt install ./target/debian/ndiclient_0.3.0-1+deb13_arm64.deb
```

The package version comes from `packaging/debian/changelog` and must match
the version in `Cargo.toml`. `+deb13` (or `+deb12`) names the Debian release
it was built on. The package installs `/usr/bin/ndiclient` and
its man page, and bundles the NDI runtime
library privately in `/usr/lib/ndiclient`, as the NDI SDK documentation
asks. It recommends `avahi-daemon`, which NDI discovery uses for mDNS.


## License

This code is made available with the [MIT License](./LICENSE).

This code uses the NDI SDK, whose terms are available at:
[https://docs.ndi.video/all/developing-with-ndi/sdk/licensing](https://docs.ndi.video/all/developing-with-ndi/sdk/licensing)
