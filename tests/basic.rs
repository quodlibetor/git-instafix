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
fn test_straightforward() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    git_file_commit("a", &td);
    git_file_commit("b", &td);
    git(&["checkout", "-b", "changes", "HEAD~"], &td);
    git(&["branch", "-u", "main"], &td);
    for n in &["c", "d", "e"] {
        git_file_commit(&n, &td);
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

    fixup(&td).args(&["-P", "d"]).output().unwrap();

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
fn simple_straightline_commits() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    for n in &["a", "b"] {
        git_file_commit(n, &td);
    }
    git(&["checkout", "-b", "changes"], &td);
    git(&["branch", "-u", "main"], &td);
    for n in &["target", "d"] {
        git_file_commit(&n, &td);
    }

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

    fixup(&td).args(&["-P", "target"]).assert().success();

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
fn test_no_commit_in_range() {
    let td = assert_fs::TempDir::new().unwrap();
    git_init(&td);

    for n in &["a", "b", "c", "d"] {
        git_file_commit(n, &td);
    }
    git(&["checkout", "-b", "changes", ":/c"], &td);
    git(&["branch", "-u", "main"], &td);
    for n in &["target", "f", "g"] {
        git_file_commit(&n, &td);
    }

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

    let assertion = fixup(&td).args(&["-P", "b"]).assert().failure();
    let out = string(assertion.get_output().stdout.clone());
    assert!(out.contains("No commit contains the pattern"), out);

    fixup(&td).args(&["-P", "target"]).assert().success();

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

///////////////////////////////////////////////////////////////////////////////
// Helpers

fn git_init(tempdir: &assert_fs::TempDir) {
    git(&["init", "--initial-branch=main"], &tempdir);
    git(&["config", "user.email", "nobody@nowhere.com"], &tempdir);
    git(&["config", "user.name", "nobody"], &tempdir);
}

/// Create a file and commit it with a mesage that is just the name of the file
fn git_file_commit(name: &str, tempdir: &assert_fs::TempDir) {
    tempdir.child(format!("file_{}", name)).touch().unwrap();
    git(&["add", "-A"], &tempdir);
    git(&["commit", "-m", &name], &tempdir);
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
        &tempdir,
    );
    (string(out.stdout), string(out.stderr))
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
        git_inner(&["log", "--all", "--format=%s %D", "--graph"], &tempdir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .lines()
    .map(|l| l.trim_end())
    .join("\n");
    s.push_str("\n");
    s
}

fn string(from: Vec<u8>) -> String {
    String::from_utf8(from).unwrap()
}

fn git_inner(args: &[&str], tempdir: &assert_fs::TempDir) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(&tempdir.path());
    cmd
}

/// Get something that can get args added to it
fn fixup(dir: &assert_fs::TempDir) -> Command {
    let mut c = Command::cargo_bin("git-fixup").unwrap();
    c.current_dir(&dir.path());
    c
}
