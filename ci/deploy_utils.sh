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
    local tmpdir out_dir name staging
    # Create a temporary dir that contains our staging area.
    # $tmpdir/$name is what eventually ends up as the deployed archive.
    tmpdir="$(mk_temp)"
    name="${PROJECT_NAME}-${RELEASE}-${TARGET}"
    staging="$tmpdir/$name"

    # The deployment directory is where the final archive will reside.
    # This path is known by the github actions configuration.
    out_dir="$(pwd)/deployment"
    mkdir -p "$staging" "$out_dir"

    cp "target/$TARGET/release/git-fixup" "$staging/git-fixup"
    cp README.md LICENSE-{MIT,APACHE} "$staging/"

    (cd "$tmpdir" && tar -czf "$out_dir/$name.tar.gz" "$name")
    rm -rf "$tmpdir"

    ls "$out_dir/$name.tar.gz"
}
