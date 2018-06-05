#!/bin/bash

# install stuff needed for the `script` phase

# Where rustup gets installed.
export PATH="$PATH:$HOME/.cargo/bin"

set -ex

. "$(dirname "$0")/utils.sh"

install_rustup() {
    curl https://sh.rustup.rs -sSf \
      | sh -s -- -y --default-toolchain="$TRAVIS_RUST_VERSION"
    rustc -V
    cargo -V
}

install_targets() {
    if [[ $(host) != "$TARGET" ]]; then
        rustup target add "$TARGET"
    fi
}

install_deps() {
    if is_musl; then
        echo "Building OpenSSL"
        cd /tmp
        OPENSSL_VERSION=1.0.2o
        curl -LO "https://www.openssl.org/source/openssl-$OPENSSL_VERSION.tar.gz"
        tar xzf "openssl-$OPENSSL_VERSION.tar.gz"
        cd "openssl-$OPENSSL_VERSION"
        env CC=musl-gcc ./Configure no-shared no-zlib -fPIC --prefix=/usr/local/musl linux-x86_64
        env C_INCLUDE_PATH=/usr/local/musl/include/ make depend
        make
        make install

        echo "Building zlib"
        cd /tmp
        ZLIB_VERSION=1.2.11
        curl -LO "http://zlib.net/zlib-$ZLIB_VERSION.tar.gz"
        tar xzf "zlib-$ZLIB_VERSION.tar.gz"
        cd "zlib-$ZLIB_VERSION"
        CC=musl-gcc ./configure --static --prefix=/usr/local/musl
        make
        sudo make install
    fi
}

configure_cargo() {
    local prefix=$(gcc_prefix)
    if [ -n "${prefix}" ]; then
        local gcc_suffix=
        if [ -n "$GCC_VERSION" ]; then
          gcc_suffix="-$GCC_VERSION"
        fi
        local gcc="${prefix}gcc${gcc_suffix}"

        # information about the cross compiler
        "${gcc}" -v

        # tell cargo which linker to use for cross compilation
        mkdir -p .cargo
        cat >>.cargo/config <<EOF
[target.$TARGET]
linker = "${gcc}"
EOF
    fi
}

main() {
    install_rustup
    install_targets
    install_deps
    configure_cargo
}

main
