#!/bin/sh
# Builds target/debian/ndiclient_<version>_<arch>.deb.
#
# The NDI runtime is bundled privately in /usr/lib/ndiclient, as the NDI SDK
# documentation asks: applications keep their own copy instead of installing
# it on the system path. Needs the NDI SDK (NDI_SDK_DIR), cargo and
# dpkg-dev (for dpkg-shlibdeps and dpkg-parsechangelog).
#
# The package version is the latest entry in packaging/debian/changelog. Its
# upstream part must match the version in Cargo.toml.
set -eu

PACKAGE_NAME=ndiclient
SDK_DIR="${NDI_SDK_DIR:-/home/pi/NDI SDK for Linux}"
SDK_LIBRARY_DIR="$SDK_DIR/lib/aarch64-rpi4-linux-gnueabi"
NDI_LIBRARY=libndi.so.6
INSTALLED_BINARY_DIR=/usr/bin
INSTALLED_RUNTIME_DIR=/usr/lib/$PACKAGE_NAME
INSTALLED_DOC_DIR=/usr/share/doc/$PACKAGE_NAME
INSTALLED_MAN_DIR=/usr/share/man/man1

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PACKAGING_DIR="$PROJECT_DIR/packaging/debian"
# A separate target directory keeps the packaged binary (whose rpath points
# at the installed runtime) apart from the development build in target/release.
CARGO_TARGET_DIR="$PROJECT_DIR/target/debian/cargo"
OUTPUT_DIR="$PROJECT_DIR/target/debian"

main() {
    version="$(package_version)"
    architecture="$(dpkg --print-architecture)"
    staging_dir="$OUTPUT_DIR/${PACKAGE_NAME}_${version}_${architecture}"

    build_binary
    stage_files "$staging_dir"
    write_control_file "$staging_dir" "$version" "$architecture"
    write_md5sums "$staging_dir"
    build_package "$staging_dir"
}

package_version() {
    version="$(dpkg-parsechangelog -l "$PACKAGING_DIR/changelog" -S Version)"
    cargo_version="$(cargo pkgid --manifest-path "$PROJECT_DIR/Cargo.toml" | sed 's/.*[#@]//')"
    if [ "${version%-*}" != "$cargo_version" ]; then
        echo "changelog version $version does not match Cargo.toml version $cargo_version" >&2
        exit 1
    fi
    echo "$version"
}

build_binary() {
    NDI_SDK_DIR="$SDK_DIR" NDI_RUNTIME_DIR="$INSTALLED_RUNTIME_DIR" \
        cargo build --release --locked \
        --manifest-path "$PROJECT_DIR/Cargo.toml" \
        --target-dir "$CARGO_TARGET_DIR"
}

stage_files() {
    root="$1"
    rm -rf "$root"
    install -d "$root/DEBIAN"
    install -D -m 0755 -s "$CARGO_TARGET_DIR/release/$PACKAGE_NAME" \
        "$root$INSTALLED_BINARY_DIR/$PACKAGE_NAME"
    install -D -m 0644 "$SDK_LIBRARY_DIR/$NDI_LIBRARY" \
        "$root$INSTALLED_RUNTIME_DIR/$NDI_LIBRARY"
    install -D -m 0644 "$PACKAGING_DIR/copyright" "$root$INSTALLED_DOC_DIR/copyright"
    install -m 0644 "$PROJECT_DIR/README.md" "$root$INSTALLED_DOC_DIR/README.md"
    install -m 0644 "$SDK_DIR/NDI SDK License Agreement.txt" "$root$INSTALLED_DOC_DIR/"
    install -m 0644 "$SDK_DIR/licenses/libndi_licenses.txt" "$root$INSTALLED_DOC_DIR/"
    install_compressed "$PACKAGING_DIR/changelog" "$root$INSTALLED_DOC_DIR/changelog.Debian.gz"
    install_compressed "$PACKAGING_DIR/$PACKAGE_NAME.1" "$root$INSTALLED_MAN_DIR/$PACKAGE_NAME.1.gz"
}

# -n leaves out the file name and time stamp, so rebuilds are reproducible.
install_compressed() {
    install -d "$(dirname "$2")"
    gzip -9 -n -c "$1" > "$2"
    chmod 0644 "$2"
}

write_control_file() {
    root="$1"
    installed_size="$(installed_size_kib "$root")"
    depends="$(shared_library_dependencies "$root")"
    sed -e "s|@VERSION@|$2|" \
        -e "s|@ARCHITECTURE@|$3|" \
        -e "s|@INSTALLED_SIZE@|$installed_size|" \
        -e "s|@DEPENDS@|$depends|" \
        "$PACKAGING_DIR/control.in" > "$root/DEBIAN/control"
}

installed_size_kib() {
    du -sk --exclude=DEBIAN "$1" | cut -f1
}

# dpkg-shlibdeps reads debian/control from the working directory, so it runs
# in a scratch directory holding a minimal one.
shared_library_dependencies() {
    root="$1"
    scratch_dir="$OUTPUT_DIR/shlibdeps"
    rm -rf "$scratch_dir"
    mkdir -p "$scratch_dir/debian"
    printf 'Source: %s\n\nPackage: %s\nArchitecture: any\n' \
        "$PACKAGE_NAME" "$PACKAGE_NAME" > "$scratch_dir/debian/control"
    substitution="$(cd "$scratch_dir" && dpkg-shlibdeps -O \
        -l"$root$INSTALLED_RUNTIME_DIR" \
        "$root$INSTALLED_BINARY_DIR/$PACKAGE_NAME" \
        "$root$INSTALLED_RUNTIME_DIR/$NDI_LIBRARY")"
    rm -rf "$scratch_dir"
    echo "${substitution#shlibs:Depends=}"
}

write_md5sums() {
    root="$1"
    (cd "$root" && find . -path ./DEBIAN -prune -o -type f -printf '%P\0' \
        | sort -z | xargs -0 md5sum) > "$root/DEBIAN/md5sums"
}

build_package() {
    dpkg-deb --root-owner-group --build "$1" "$OUTPUT_DIR"
}

main "$@"
