#!/bin/sh
# Adds .deb packages to one distribution of a static APT repository and
# re-signs that distribution's metadata.
#
# Usage: APT_SIGNING_KEY=<key id> packaging/update_apt_repo.sh <repo-dir> <distribution> <deb>...
#
# Each distribution (the Debian release the packages were built on, e.g.
# trixie or bookworm) has one component and its own pool directory, so its
# index lists only the packages built for it. Packages already in a pool
# stay, so older versions remain installable. A package file that is already
# there with different content is refused: APT caches packages by name and
# version, so a rebuilt version needs a new changelog entry. Needs apt-utils
# (for apt-ftparchive) and gnupg with the signing key in its keyring.
set -eu

COMPONENT=main
ORIGIN=ndiclient
PACKAGE_SUBDIR=$COMPONENT/n/ndiclient
KEYRING_NAME=ndiclient-archive-keyring
SIGNING_KEY="${APT_SIGNING_KEY:?APT_SIGNING_KEY must name the signing key}"
# Until bookworm was added, trixie was the only distribution and its pool was
# pool/main. adopt_legacy_pool moves it to pool/trixie/main.
LEGACY_DISTRIBUTION=trixie

main() {
    REPO_DIR="$1"
    DISTRIBUTION="$2"
    POOL="pool/$DISTRIBUTION"
    shift 2
    adopt_legacy_pool
    add_packages "$@"
    for architecture in $(pool_architectures); do
        write_package_index "$architecture"
    done
    write_release
    sign_release
    export_public_key
}

adopt_legacy_pool() {
    legacy_pool="$REPO_DIR/pool/$COMPONENT"
    if [ "$DISTRIBUTION" = "$LEGACY_DISTRIBUTION" ] && [ -d "$legacy_pool" ]; then
        install -d "$REPO_DIR/$POOL"
        mv "$legacy_pool" "$REPO_DIR/$POOL/$COMPONENT"
    fi
}

add_packages() {
    pool_dir="$REPO_DIR/$POOL/$PACKAGE_SUBDIR"
    install -d "$pool_dir"
    for package in "$@"; do
        target="$pool_dir/$(basename "$package")"
        if [ -e "$target" ] && ! cmp -s "$package" "$target"; then
            echo "$target already exists with different content" >&2
            exit 1
        fi
        install -m 0644 "$package" "$target"
    done
}

pool_architectures() {
    find "$REPO_DIR/$POOL" -name '*.deb' -exec dpkg-deb --field {} Architecture \; | sort -u
}

# apt-ftparchive runs in the repository root, so the index lists pool paths
# relative to it, as APT expects.
write_package_index() {
    index_dir="$REPO_DIR/dists/$DISTRIBUTION/$COMPONENT/binary-$1"
    install -d "$index_dir"
    (cd "$REPO_DIR" && apt-ftparchive --arch "$1" packages "$POOL") > "$index_dir/Packages"
    gzip -9 -n -k -f "$index_dir/Packages"
}

# The old Release files are removed first and the new one is written
# elsewhere, so neither is hashed into the new one.
write_release() {
    release_dir="$REPO_DIR/dists/$DISTRIBUTION"
    rm -f "$release_dir/Release" "$release_dir/InRelease" "$release_dir/Release.gpg"
    architectures="$(pool_architectures | tr '\n' ' ')"
    new_release="$(mktemp)"
    apt-ftparchive \
        -o APT::FTPArchive::Release::Origin="$ORIGIN" \
        -o APT::FTPArchive::Release::Label="$ORIGIN" \
        -o APT::FTPArchive::Release::Suite="$DISTRIBUTION" \
        -o APT::FTPArchive::Release::Codename="$DISTRIBUTION" \
        -o APT::FTPArchive::Release::Components="$COMPONENT" \
        -o APT::FTPArchive::Release::Architectures="${architectures% }" \
        release "$release_dir" > "$new_release"
    install -m 0644 "$new_release" "$release_dir/Release"
    rm -f "$new_release"
}

sign_release() {
    release="$REPO_DIR/dists/$DISTRIBUTION/Release"
    gpg --batch --yes --local-user "$SIGNING_KEY" --clearsign \
        --output "$(dirname "$release")/InRelease" "$release"
    gpg --batch --yes --local-user "$SIGNING_KEY" --armor --detach-sign \
        --output "$release.gpg" "$release"
}

# The binary keyring goes to /usr/share/keyrings for signed-by; the armored
# copy is for people who want to read the key before trusting it.
export_public_key() {
    gpg --batch --yes --export --output "$REPO_DIR/$KEYRING_NAME.gpg" "$SIGNING_KEY"
    gpg --batch --yes --armor --export --output "$REPO_DIR/$KEYRING_NAME.asc" "$SIGNING_KEY"
}

main "$@"
