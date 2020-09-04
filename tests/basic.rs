use assert_cmd::Command;
use assert_fs::prelude::*;
use itertools::Itertools;

#[test]
fn test_can_compile() {
    let mut cmd = fixup();
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
    let out = git_log(&td);
    assert_eq!(
        out,
        "\
* b HEAD -> main
* a
",
        "log:\n{}",
        out
    );

    git(&["checkout", "-b", "changes", "HEAD~"], &td);
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

    let cmd = fixup().write_stdin("q\n\n\n").unwrap();
    let err = string(cmd.stderr);

    assert_eq!(err, "nope", "err: {}", err);
}

fn git_init(tempdir: &assert_fs::TempDir) {
    git(&["init", "--initial-branch=main"], &tempdir);
}

/// Create a file and commit it with a mesage that is just the name of the file
fn git_file_commit(name: &str, tempdir: &assert_fs::TempDir) {
    tempdir.child(name).touch().unwrap();
    git(&["add", "-A"], &tempdir);
    git(&["commit", "-m", &name], &tempdir);
}

/// Run git in tempdir with args and panic if theres an error
fn git(args: &[&str], tempdir: &assert_fs::TempDir) {
    git_inner(args, tempdir).ok().unwrap();
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
fn fixup() -> Command {
    Command::cargo_bin("git-fixup").unwrap()
}
