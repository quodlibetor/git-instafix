# git fixup

Quickly fix up an old commit using your currently-staged changes.

[![asciicast](./static/asciicast.png)](https://asciinema.org/a/bLZ1eFaDTKKvVMtPzgTUgYNVG)

## Installation

If you're on macos or linux and using homebrew you should be able to do:

    brew install quodlibetor/git-fixup/git-fixup

Otherwise, you will need to compile with Rust. Install rust, clone this repo,
build, and then copy the binary into your bin dir:

    curl https://sh.rustup.rs -sSf | sh
    git clone https://github.com/quodlibetor/git-fixup && cd git-fixup
    cargo build --release
    cp target/release/git-fixup /usr/local/bin/git-fixup

## Usage

After installation, just run `git fixup` or `git squash` to perform the related actions.

## License

git-fixup is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

Patches and bug reports welcome!
