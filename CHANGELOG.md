# Unreleased

# Version 0.2.1

* Remove last dependency on external git binary, using libgit2 for all git interactions
* Show backtraces on error if RUST_BACKTRACE=1 is in the environment
* Correctly stash and unstash changes before the rebase

# Version 0.2.0

* Rename to git-instafix because there are a bunch of existing projects named git-fixup

# Version 0.1.9

* CI and doc improvements
* Use libgit2 instead of shelling out for more things.
* Create binaries and install scripts with cargo-dist
