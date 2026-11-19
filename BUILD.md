# Building & Packaging

## Building from Source

Currently `ndiclient` requires `libsdl2-dev` and the NDI SDK available at
[https://ndi.video/for-developers/ndi-sdk/](https://ndi.video/for-developers/ndi-sdk/).
The environment var `NDI_SDK_DIR` can be set to the path of the NDI SDK,
it defaults to `/home/pi/NDI SDK for Linux` otherwise.

After those are installed the build should obey Cargo:
```sh
cargo build --release
```

## Testing

A test pattern sender is available in the `tools/` directory for local integration testing.

Unit testing can be performed using:
```sh
cargo test
```

## Creating Debian Packages

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
