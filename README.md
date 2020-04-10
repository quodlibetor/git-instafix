# git fixup

Quickly fix up an old commit using your currently-staged changes.

![usage](./static/full-workflow-simple.gif)

## Usage

After installation, just run `git fixup` or `git squash` to perform the related
actions.

By default, `git fixup` checks for staged changes and offers to amend an old
commit.

Given a repo that looks like:

![linear-repo](./static/00-initial-state.png)

Running `git fixup` will allow you to edit an old commit:

![linear-repo-fixup](./static/01-selector.gif)

The default behavior will check if your current HEAD commit has an `upstream`
branch and show you only the commits between where you currently are and that
commit. If there is no upstream for HEAD you will see the behavior above.

If you're using a pull-request workflow (e.g. github) you will often have repos that look more like this:

![full-repo](./static/20-initial-full-repo.png)

You can set `GIT_INSTAFIX_UPSTREAM` to a branch name and `git fixup` will only
show changes between HEAD and the merge-base:

![full-repo-fixup](./static/21-with-upstream.gif)

In general this is just what you want, since you probably shouldn't be editing
commits that other people are working off of.

After you select the commit to edit, `git fixup` will apply your staged changes
to that commit without any further prompting or work from you.

`git-squash` is just a symlink to `git-fixup` installed by brew, but if you
invoke it (either as `git-squash` or `git squash`) it will behave the same,
asking you which change to amend, but after you have selected the commit to git
will give you a chance to edit the commit message before changing the tree at
that point.

## Installation

If you're on macos or linux and using homebrew you should be able to do:

    brew install quodlibetor/git-fixup/git-fixup

Otherwise, you will need to compile with Rust. Install rust, clone this repo,
build, and then copy the binary into your bin dir:

    curl https://sh.rustup.rs -sSf | sh
    git clone https://github.com/quodlibetor/git-fixup && cd git-fixup
    cargo build --release
    cp target/release/git-fixup /usr/local/bin/git-fixup

## License

git-fixup is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

Patches and bug reports welcome!
