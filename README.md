# ndiclient - NDI Video Monitor for Linux

An NDI video monitor intended for Raspbian-like distributions.
I couldn't find another CLI that allowed me to stream NDI sources,
so this is my attempt at bootstrapping one.

NDI® is a registered trademark of Vizrt NDI AB. Learn more at [ndi.video](https://ndi.video/).


## Current State

This currently is a bootstrap PoC written with Claude Code,
goal is to make this a hybrid human/LLM maintained codebase with
an emphasis on maintainability and simplicity.

Sources are found with NDI discovery. Audio is not supported yet -
that will be coming later.


## Building

Currently `ndiclient` requires `libsdl2-dev` and the NDI SDK available at
[https://ndi.video/for-developers/ndi-sdk/](https://ndi.video/for-developers/ndi-sdk/).
The environment var `NDI_SDK_DIR` can be set to the path of the NDI SDK,
it defaults to `/home/pi/NDI SDK for Linux` otherwise.

After those are installed the build should obey Cargo:
```sh
cargo build --release
```


## Debian Package

To build a `.deb` for the local machine's architecture, run:
```sh
packaging/build_deb.sh
```

This needs `dpkg-dev` (for `dpkg-shlibdeps`) in addition to the build
requirements above. The package lands in `target/debian/` and installs with:
```sh
sudo apt install ./target/debian/ndiclient_0.1.0-1_arm64.deb
```

The package version comes from `packaging/debian/changelog` and must match
the version in `Cargo.toml`. The package installs `/usr/bin/ndiclient` and
its man page, and bundles the NDI runtime
library privately in `/usr/lib/ndiclient`, as the NDI SDK documentation
asks. It recommends `avahi-daemon`, which NDI discovery uses for mDNS.


## Installing from the APT Repository

Released packages for Raspberry Pi OS "trixie" (arm64) are published as a
signed APT repository on GitHub Pages. To add it and install `ndiclient`:
```sh
sudo curl -fsSLo /usr/share/keyrings/ndiclient-archive-keyring.gpg \
    https://deckerego.github.io/ndiclient/ndiclient-archive-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/ndiclient-archive-keyring.gpg] https://deckerego.github.io/ndiclient trixie main" \
    | sudo tee /etc/apt/sources.list.d/ndiclient.list
sudo apt update
sudo apt install ndiclient
```

Later releases then arrive with `sudo apt upgrade`.

To publish a release, bump the version in `Cargo.toml`, add a matching entry
to `packaging/debian/changelog`, merge, then tag and push:
```sh
git tag v0.1.0
git push origin v0.1.0
```

The `APT repository` workflow builds the package and adds it to the
`gh-pages` branch. Pull requests only build it.


## Usage

Run without arguments to list the NDI sources discovered on the network:
```sh
target/release/ndiclient
```

Then pass the NDI device name and stream name of a source. NDI lists sources
as `DEVICE (Stream)`, so for the source `STUDIO (OBS PGM)`:
```sh
target/release/ndiclient STUDIO "OBS PGM"
```

The stream name is optional. Without it, the first stream discovered on that
device is shown:
```sh
target/release/ndiclient STUDIO
```

Add `--fullscreen` to show the video fullscreen instead of in a window:
```sh
target/release/ndiclient --fullscreen STUDIO "OBS PGM"
```

In fullscreen the mouse cursor is hidden, so the screen shows only the video
(kiosk mode). Press F11 or F to switch between window and fullscreen while
the video plays. Close the window or press Escape to quit.

The device name is matched ignoring case; the stream name must match exactly.
If no matching source is discovered within 10 seconds, the error lists the
sources that were discovered.


## Performance Hacks

Turning off wifi power saving may help a bit with packet loss. 
Run `sudo iw dev wlan0 set power_save off` to try it out.


## Testing

A test pattern sender is available in the `tools/` directory for local integration testing.

Unit testing can be performed using:
```sh
cargo test
```


## Limitations

*  Right now the receiver requests the highest bandwidth feed available, low bandwidth mode is not supported.
*  Audio is currently not supported, but will be added before the first stable release.
*  No resolution scaling is provided yet.
*  Right now the target platform is the Raspberry Pi AArch64 platform and tested on an RPi 5 4GB. In theory this arm64 package could work on the RPi 3 or newer (including the RPi Zero 2), but this hasn't been validated yet. Other Linux distros or platforms are currently not available.


## License

This code is made available with the [MIT License](./LICENSE).

This code uses the NDI SDK, whose terms are available at:
[https://docs.ndi.video/all/developing-with-ndi/sdk/licensing](https://docs.ndi.video/all/developing-with-ndi/sdk/licensing)
