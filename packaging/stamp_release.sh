#!/bin/sh
# Stamps a release into the source tree: sets the version in Cargo.toml and
# Cargo.lock and adds a packaging/debian/changelog entry made from the
# release notes, read from standard input.
#
#   stamp_release.sh <version> <release date> < release-notes.md
#
# The version is the release tag, X.Y.Z without a "v". The date is anything
# `date -d` reads (GitHub gives ISO 8601). Each non-blank line of the notes
# becomes one changelog item, with Markdown list and heading markers removed.
# If the changelog's latest entry already has this version, it is kept as
# it is, so stamping twice changes nothing. Needs cargo and dpkg-dev.
set -eu

PACKAGE_NAME=ndiclient
DEBIAN_REVISION=1
DISTRIBUTION=unstable
URGENCY=medium
CHANGELOG_WIDTH=76
EMPTY_NOTES_ITEM="New upstream release."

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PACKAGING_DIR="$PROJECT_DIR/packaging/debian"
CHANGELOG="$PACKAGING_DIR/changelog"
CARGO_MANIFEST="$PROJECT_DIR/Cargo.toml"

main() {
    [ $# -eq 2 ] || usage
    version="$1"
    release_date="$(changelog_date "$2")"
    require_release_version "$version"

    if needs_changelog_entry "$version-$DEBIAN_REVISION"; then
        add_changelog_entry "$version-$DEBIAN_REVISION" "$release_date"
    fi
    set_cargo_version "$version"
}

usage() {
    echo "usage: $0 <version> <release date> < release-notes" >&2
    exit 2
}

changelog_date() {
    date -R -u -d "$1"
}

require_release_version() {
    if ! echo "$1" | grep -Eqx '[0-9]+\.[0-9]+\.[0-9]+'; then
        echo "release version $1 is not X.Y.Z" >&2
        exit 1
    fi
}

# Only the [package] version line starts the line with "version = ".
set_cargo_version() {
    sed -i "0,/^version = /s/^version = \".*\"/version = \"$1\"/" "$CARGO_MANIFEST"
    cargo update --workspace --manifest-path "$CARGO_MANIFEST"
}

needs_changelog_entry() {
    latest="$(dpkg-parsechangelog -l "$CHANGELOG" -S Version 2>/dev/null)"
    if dpkg --compare-versions "$1" lt "$latest"; then
        echo "release $1 is older than the changelog's latest entry $latest" >&2
        exit 1
    fi
    dpkg --compare-versions "$1" gt "$latest"
}

# The new entry goes below the comment lines that open the changelog. It
# reaches awk through the environment, which unlike -v keeps backslashes.
add_changelog_entry() {
    updated="$(mktemp)"
    ENTRY="$(changelog_entry "$1" "$2")" awk '
        !done && !/^#/ { print ENVIRON["ENTRY"]; print ""; done = 1 }
        { print }
    ' "$CHANGELOG" > "$updated"
    cat "$updated" > "$CHANGELOG"
    rm -f "$updated"
}

changelog_entry() {
    echo "$PACKAGE_NAME ($1) $DISTRIBUTION; urgency=$URGENCY"
    echo
    changelog_items
    echo
    echo " -- $(maintainer)  $2"
}

changelog_items() {
    items="$(release_note_lines)"
    printf '%s\n' "${items:-$EMPTY_NOTES_ITEM}" | while IFS= read -r item; do
        printf '%s\n' "$item" | fold -s -w "$CHANGELOG_WIDTH" \
            | sed -e 's/[[:space:]]*$//' -e '1s/^/  * /' -e '1!s/^/    /'
    done
}

# GitHub stores release notes with CRLF line ends.
release_note_lines() {
    tr -d '\r' \
        | sed -E -e 's/^[[:space:]]*([-*+]|[0-9]+[.)]|#+)[[:space:]]+//' \
                 -e 's/[[:space:]]+$//' \
        | grep -v '^$' || true
}

maintainer() {
    sed -n 's/^Maintainer: //p' "$PACKAGING_DIR/control.in"
}

main "$@"
