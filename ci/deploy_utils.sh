#!/bin/bash

# outputs the temporary directory created
mk_temp() {
    # on old versions of macos mktemp is... annoying.
    # -t is deprecated on all recent OSs
    if [[ $TARGET = *darwin ]]; then
        mktemp -t -d tmp-deploy-build.XXXXXXXXXX
    else
        mktemp -d
    fi
}

mk_tarball() {
    # When cross-compiling, use the right `strip` tool on the binary.
    local gcc_prefix="$(gcc_prefix)"
    # Create a temporary dir that contains our staging area.
    # $tmpdir/$name is what eventually ends up as the deployed archive.
    local tmpdir="$(mk_temp)"
    local name="${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}"
    local staging="$tmpdir/$name"
    # The deployment directory is where the final archive will reside.
    # This path is known by the .travis.yml configuration.
    local out_dir="$(pwd)/deployment"
    mkdir -p "$staging" "$out_dir"

    # Copy the binary and strip it.
    cp "target/$TARGET/release/git-fixup" "$staging/git-fixup"
    # Copy the licenses and README.
    cp README.md LICENSE-{MIT,APACHE} "$staging/"

    (cd "$tmpdir" && tar -czf "$out_dir/$name.tar.gz" "$name")
    rm -rf "$tmpdir"
}
