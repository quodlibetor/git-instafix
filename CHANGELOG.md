# Unreleased

# Version 0.2.7

- Support arbitrary refs (i.e. tags like `v0.1.0` and full refspecs
  like `ref/pull/ID`) as the merge-base selector.

# Version 0.2.6

- Fix and improve the experience of working with a main-only workflow.
  - Provide a tailored error message if your current branch is the selected upstream branch
  - Work correctly with explicitly-defined remote upstream branches.

# Version 0.2.5

- Correctly find git repos in parent dirs of CWD
- Enable experimental github release attestations in cargo-dist

# Version 0.2.4

- Retarget multiple branches pointing at the same commit, eg:
  > updated branch my-cool-branch: deadbeef -> c0ffee

# Version 0.2.3

- Allow setting the diff theme
- Read configuration from git config as well as arguments and env vars
- Choose whether to display full diff or just a diffstat based on terminal
  height instead of a constant
- Add -u alias for --default-upstream-branch

# Version 0.2.2

- Correctly retarget branches if the target of the edit is also a branch (#24)
- Check if main, master, develop, or trunk exist as reasonable default upstream branches
- Leave the repo in a less confusing state if the edit target is a conflict

# Version 0.2.1

- Remove last dependency on external git binary, using libgit2 for all git interactions
- Show backtraces on error if RUST_BACKTRACE=1 is in the environment
- Correctly stash and unstash changes before the rebase

# Version 0.2.0

- Rename to git-instafix because there are a bunch of existing projects named git-fixup

# Version 0.1.9

- CI and doc improvements
- Use libgit2 instead of shelling out for more things.
- Create binaries and install scripts with cargo-dist
