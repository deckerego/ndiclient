# ndiclient - NDI Video Monitor for Linux

An NDI video monitor intended for Raspbian-like distributions.
I couldn't find another CLI that allowed me to stream NDI sources,
so this is my attempt at bootstrapping one.

NDI® is a registered trademark of Vizrt NDI AB. Learn more at [ndi.video](https://ndi.video/).


## Current State

This currently is a bootstrap PoC written with Claude Code,
goal is to make this a hybrid human/LLM maintained codebase with
an emphasis on maintainability and simplicity.

Currently both audio and source discovery is not supported -
those will be coming later.


## Building

Currently `ndiclient` requires `libsdl2-dev` and the NDI SDK available at
[https://ndi.video/for-developers/ndi-sdk/](https://ndi.video/for-developers/ndi-sdk/).
After those are installed the build should obey Cargo:
```sh
cargo build --release
```


## Usage

Since discovery is not yet enabled I just am using an IP address for the source
and the source name as arguments:
```sh
target/release/ndiclient 192.168.1.20
target/release/ndiclient 192.168.1.20 "OBS PGM"
```


## Future State

Goal is to have this operate as an NDI video monitor without a running desktop environment,
operating full screen and scaling to the specified resolution.
Will primarily be testing audio over HDMI out, but any audio engine destination should work.


## License

This code is made available with the [MIT License](./LICENSE).

This code uses the NDI SDK, whose terms are available at:
[https://docs.ndi.video/all/developing-with-ndi/sdk/licensing](https://docs.ndi.video/all/developing-with-ndi/sdk/licensing)
