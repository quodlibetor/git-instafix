use std::process::Output;

use assert_cmd::Command;
use assert_fs::prelude::*;
use itertools::Itertools;

#[test]
fn test_can_compile() {
    let td = assert_fs::TempDir::new().unwrap();
    let mut cmd = fixup(&td);
    let ex = cmd.arg("--help").output().unwrap();
    let out = String::from_utf8(ex.stdout).unwrap();
    let err = String::from_utf8(ex.stderr).unwrap();
    assert!(
        out.contains("Fix a commit in your history with your currently-staged changes"),
        "out={} err='{}'",
        out,
        err
    );
}

#[test]
fn straightforward() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    git_file_commit("a", &td);
    git_file_commit("b", &td);
    git(&["checkout", "-b", "changes", "HEAD~"], &td);
    for n in &["c", "d", "e"] {
        git_file_commit(n, &td);
    }

    let out = git_log(&td);
    assert_eq!(
        out,
        "\
* e HEAD -> changes
* d
* c
| * b main
|/
* a
",
        "log:\n{}",
        out
    );

    td.child("new").touch().unwrap();
    git(&["add", "new"], &td);

    fixup(&td).args(["-P", "d"]).output().unwrap();

    let shown = git_out(
        &["diff-tree", "--no-commit-id", "--name-only", "-r", ":/d"],
        &td,
    );
    let files = string(shown.stdout);
    let err = string(shown.stderr);

    assert_eq!(
        files,
        "\
file_d
new
",
        "out: {} err: {}",
        files,
        err
    );
}

#[test]
fn uses_merge_base_for_all_defaults() {
    for branch in ["main", "develop", "trunk", "master"] {
        eprintln!("testing branch {branch}");
        let td = assert_fs::TempDir::new().unwrap();
        git_init_default_branch_name(branch, &td);

        git_commits(&["a", "b", "c", "d"], &td);
        git(&["checkout", "-b", "changes", ":/c"], &td);
        git_commits(&["f", "g"], &td);

        let expected = format!(
            "\
* g HEAD -> changes
* f
| * d {branch}
|/
* c
* b
* a
"
        );
        let actual = git_log(&td);
        assert_eq!(
            expected, actual,
            "expected:\n{}\nactual:\n{}",
            expected, actual
        );

        // commits *before* the merge base of a default branch don't get found by
        // default
        td.child("new").touch().unwrap();
        git(&["add", "new"], &td);
        fixup(&td).args(["-P", "b"]).assert().failure();
        // commits *after* the merge base of a default branch *do* get found by default
        git(&["reset", "HEAD~"], &td);
        git(&["add", "new"], &td);
        fixup(&td).args(["-P", "f"]).assert().success();
    }
}

#[test]
fn simple_straightline_commits() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    git_commits(&["a", "b"], &td);
    git(&["checkout", "-b", "changes"], &td);
    git(&["branch", "-u", "main"], &td);
    git_commits(&["target", "d"], &td);

    let log = git_log(&td);
    assert_eq!(
        log,
        "\
* d HEAD -> changes
* target
* b main
* a
",
        "log:\n{}",
        log
    );

    td.child("new").touch().unwrap();
    git(&["add", "new"], &td);

    fixup(&td).args(["-P", "target"]).assert().success();

    let (files, err) = git_changed_files("target", &td);

    assert_eq!(
        files,
        "\
file_target
new
",
        "out: {} err: {}",
        files,
        err
    );
}

#[test]
fn simple_straightline_tag_and_reference() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    let base = "v0.1.0";

    git_commits(&["a", "b"], &td);
    git(&["tag", base], &td);
    git(&["checkout", "-b", "changes"], &td);
    git_commits(&["target", "d"], &td);

    let log = git_log(&td);
    assert_eq!(
        log,
        "\
* d HEAD -> changes
* target
* b tag: v0.1.0, main
* a
",
        "log:\n{}",
        log
    );

    td.child("new").touch().unwrap();
    git(&["add", "new"], &td);

    fixup(&td)
        .args(["-P", "target", "-u", base])
        .assert()
        .success();

    let (files, err) = git_changed_files("target", &td);

    assert_eq!(
        files,
        "\
file_target
new
",
        "out: {} err: {}",
        files,
        err
    );

    // also check that we can use the full refspec definition

    td.child("new-full-ref").touch().unwrap();
    git(&["add", "new-full-ref"], &td);
    fixup(&td)
        .args(["-P", "target", "-u", &format!("refs/tags/{base}")])
        .unwrap();

    let (files, err) = git_changed_files("target", &td);
    assert_eq!(
        files,
        "\
file_target
new
new-full-ref
",
        "out: {} err: {}",
        files,
        err
    );
}

#[test]
fn simple_straightline_remote_branch() {
    let remote_td = assert_fs::TempDir::new().unwrap();
    git_init(&remote_td);
    git_commits(&["a", "b"], &remote_td);

    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);
    let remote_path = &remote_td
        .path()
        .as_os_str()
        .to_owned()
        .into_string()
        .unwrap();
    git(&["remote", "add", "origin", remote_path], &td);
    git(&["pull", "origin", "main:main"], &td);
    git_commits(&["target", "d"], &td);

    let log = git_log(&td);
    assert_eq!(
        log,
        "\
* d HEAD -> main
* target
* b origin/main
* a
",
        "log:\n{}",
        log
    );

    td.child("new").touch().unwrap();
    git(&["add", "new"], &td);

    fixup(&td)
        .args(["-P", "target", "-u", "origin/main"])
        .assert()
        .success();

    let (files, err) = git_changed_files("target", &td);

    assert_eq!(
        files,
        "\
file_target
new
",
        "out: {} err: {}",
        files,
        err
    );
}

#[test]
fn stashes_before_rebase() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    git_commits(&["a", "b"], &td);
    git(&["checkout", "-b", "changes"], &td);
    git(&["branch", "-u", "main"], &td);
    git_commits(&["target", "d"], &td);

    let log = git_log(&td);
    assert_eq!(
        log,
        "\
* d HEAD -> changes
* target
* b main
* a
",
        "log:\n{}",
        log
    );

    td.child("new").touch().unwrap();

    let edited_file = "file_d";
    td.child(edited_file).write_str("somthing").unwrap();

    git(&["add", "new"], &td);
    let tracked_changed_files = git_worktree_changed_files(&td);
    assert_eq!(tracked_changed_files.trim(), edited_file);

    fixup(&td).args(["-P", "target"]).assert().success();

    let (files, err) = git_changed_files("target", &td);

    assert_eq!(
        files,
        "\
file_target
new
",
        "out: {} err: {}",
        files,
        err
    );

    let popped_stashed_files = git_worktree_changed_files(&td);
    assert_eq!(popped_stashed_files.trim(), edited_file);
}

#[test]
fn test_no_commit_in_range() {
    let td = assert_fs::TempDir::new().unwrap();
    eprintln!("tempdir: {:?}", td.path());
    git_init(&td);

    git_commits(&["a", "b", "c", "d"], &td);
    git(&["checkout", "-b", "changes", ":/c"], &td);
    git(&["branch", "-u", "main"], &td);
    git_commits(&["target", "f", "g"], &td);

    let out = git_log(&td);
    assert_eq!(
        out,
        "\
* g HEAD -> changes
* f
* target
| * d main
|/
* c
* b
* a
",
        "log:\n{}",
        out
    );

    td.child("new").touch().unwrap();
    git(&["add", "new"], &td);

    let assertion = fixup(&td).args(["-P", "b"]).assert().failure();
    let out = string(assertion.get_output().stdout.clone());
    let expected = "No commit contains the pattern";
    assert!(
        out.contains(expected),
        "expected: {}\nactual: {}",
        expected,
        out
    );

    fixup(&td).args(["-P", "target"]).assert().success();

    let (files, err) = git_changed_files("target", &td);

    assert_eq!(
        files,
        "\
file_target
new
",
        "out: {} err: {}",
        files,
        err
    );
}

#[test]
fn retarget_branches_in_range() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    git_commits(&["a", "b"], &td);
    git(&["checkout", "-b", "intermediate"], &td);
    git_commits(&["target", "c", "d"], &td);
    git(&["checkout", "-b", "points-at-intermediate"], &td);

    git(&["checkout", "-b", "changes"], &td);
    git_commits(&["e", "f"], &td);

    let expected = "\
* f HEAD -> changes
* e
* d points-at-intermediate, intermediate
* c
* target
* b main
* a
";
    let out = git_log(&td);
    assert_eq!(out, expected, "log:\n{}\nexpected:\n{}", out, expected);

    td.child("new").touch().unwrap();
    git(&["add", "new"], &td);

    fixup(&td).args(["-P", "target"]).assert().success();

    let (files, err) = git_changed_files("target", &td);

    assert_eq!(
        files,
        "\
file_target
new
",
        "out: {} err: {}",
        files,
        err
    );

    // should be identical to before
    let out = git_log(&td);
    assert_eq!(out, expected, "\nactual:\n{}\nexpected:\n{}", out, expected);
}

#[test]
fn retarget_branch_target_of_edit() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    git_commits(&["a", "b"], &td);
    git(&["checkout", "-b", "intermediate"], &td);
    git_commits(&["c", "d", "target"], &td);

    git(&["checkout", "-b", "changes"], &td);
    git_commits(&["e", "f"], &td);

    let expected = "\
* f HEAD -> changes
* e
* target intermediate
* d
* c
* b main
* a
";
    let out = git_log(&td);
    assert_eq!(
        out, expected,
        "before rebase:\nactual:\n{}\nexpected:\n{}",
        out, expected
    );

    td.child("new").touch().unwrap();
    git(&["add", "new"], &td);

    fixup(&td).args(["-P", "target"]).assert().success();

    let out = git_log(&td);
    assert_eq!(
        out, expected,
        "after rebase\nactual:\n{}\nexpected:\n{}",
        out, expected
    );

    let (files, err) = git_changed_files("target", &td);
    assert_eq!(
        files,
        "\
file_target
new
",
        "out: {} err: {}",
        files,
        err
    );

    // should be identical to before
    let out = git_log(&td);
    assert_eq!(out, expected, "\nactual:\n{}\nexpected:\n{}", out, expected);
}

///////////////////////////////////////////////////////////////////////////////
// Helpers

fn git_commits(ids: &[&str], tempdir: &assert_fs::TempDir) {
    for n in ids {
        git_file_commit(n, tempdir);
    }
}

fn git_init(tempdir: &assert_fs::TempDir) {
    git_init_default_branch_name("main", tempdir)
}

fn git_init_default_branch_name(name: &str, tempdir: &assert_fs::TempDir) {
    git(&["init", "--initial-branch", name], tempdir);
    git(&["config", "user.email", "nobody@nowhere.com"], tempdir);
    git(&["config", "user.name", "nobody"], tempdir);
}

/// Create a file and commit it with a mesage that is just the name of the file
fn git_file_commit(name: &str, tempdir: &assert_fs::TempDir) {
    tempdir.child(format!("file_{}", name)).touch().unwrap();
    git(&["add", "-A"], tempdir);
    git(&["commit", "-m", name], tempdir);
}

/// Get the git shown output for the target commit
fn git_changed_files(name: &str, tempdir: &assert_fs::TempDir) -> (String, String) {
    let out = git_out(
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            &format!(":/{}", name),
        ],
        tempdir,
    );
    (string(out.stdout), string(out.stderr))
}

fn git_worktree_changed_files(td: &assert_fs::TempDir) -> String {
    string(git_out(&["diff", "--name-only"], td).stdout)
}

/// Run git in tempdir with args and panic if theres an error
fn git(args: &[&str], tempdir: &assert_fs::TempDir) {
    git_inner(args, tempdir).ok().unwrap();
}

fn git_out(args: &[&str], tempdir: &assert_fs::TempDir) -> Output {
    git_inner(args, tempdir).output().unwrap()
}

fn git_log(tempdir: &assert_fs::TempDir) -> String {
    let mut s = String::from_utf8(
        git_inner(&["log", "--all", "--format=%s %D", "--graph"], tempdir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .lines()
    .map(|l| l.trim_end())
    .join("\n");
    s.push('\n');
    s
}

fn string(from: Vec<u8>) -> String {
    String::from_utf8(from).unwrap()
}

fn git_inner(args: &[&str], tempdir: &assert_fs::TempDir) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(tempdir.path());
    cmd
}

/// Get something that can get args added to it
fn fixup(dir: &assert_fs::TempDir) -> Command {
    let mut c = Command::cargo_bin("git-instafix").unwrap();
    c.current_dir(dir.path())
        .env_remove("GIT_INSTAFIX_UPSTREAM");
    c
}
