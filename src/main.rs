extern crate console;
extern crate dialoguer;
extern crate git2;
#[macro_use]
extern crate structopt;

use std::env;
use std::error::Error;
use std::process::Command;

use dialoguer::{Confirmation, Select};
use git2::{Branch, Commit, Diff, Repository};
use structopt::StructOpt;
use console::style;

/// Fix a commit in your history with your currently-staged changes
#[derive(StructOpt, Debug)]
struct Args {
    /// Use `squash!`: change the commit message that you amend
    #[structopt(short = "s", long = "squash")]
    squash: bool,
}

#[derive(Eq, PartialEq, Debug)]
enum Changes {
    Staged,
    Unstaged,
}

fn main() {
    let mut args = Args::from_args();
    if env::args().take(1).next().unwrap().ends_with("squash") {
        args.squash = true
    }
    if let Err(e) = run(args.squash) {
        // An empty message means don't display any error message
        let msg = e.to_string();
        if !msg.is_empty() {
            println!("Error running rebase: {}", e);
        }
    }
}

fn run(squash: bool) -> Result<(), Box<Error>> {
    let repo = Repository::open(".")?;
    match repo.head() {
        Ok(head) => {
            let head_tree = head.peel_to_tree()?;
            let head_branch = Branch::wrap(head);
            let diff = repo.diff_tree_to_index(Some(&head_tree), None, None)?;
            let commit_to_amend = create_fixup_commit(&repo, &head_branch, &diff, squash)?;
            println!(
                "selected: {} {}",
                &commit_to_amend.id().to_string()[0..10],
                commit_to_amend.summary().unwrap_or("")
            );
            // do the rebase
            let target_id = format!("{}~", commit_to_amend.id());
            Command::new("git")
                .args(&["rebase", "--interactive", "--autosquash", &target_id])
                .env("GIT_SEQUENCE_EDITOR", "true")
                .spawn()?
                .wait()?;
        }
        Err(e) => return Err(format!("head is not pointing at a valid branch: {}", e).into()),
    };
    Ok(())
}

fn create_fixup_commit<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    diff: &'a Diff,
    squash: bool,
) -> Result<Commit<'a>, Box<Error>> {
    let diffstat = diff.stats()?;
    if diffstat.files_changed() == 0 {
        print_diff(Changes::Unstaged)?;
        if !Confirmation::new("Nothing staged, stage and commit everything?").interact()? {
            return Err("".into());
        }
        let pathspecs: Vec<&str> = vec![];
        let mut idx = repo.index()?;
        idx.update_all(&pathspecs, None)?;
        idx.write()?;
        let commit_to_amend = select_commit_to_amend(&repo, head_branch.upstream().ok())?;
        do_fixup_commit(&repo, &head_branch, &commit_to_amend, squash)?;
        Ok(commit_to_amend)
    } else {
        println!("Staged changes:");
        print_diff(Changes::Staged)?;
        let commit_to_amend = select_commit_to_amend(&repo, head_branch.upstream().ok())?;
        do_fixup_commit(&repo, &head_branch, &commit_to_amend, squash)?;
        Ok(commit_to_amend)
    }
}

fn do_fixup_commit<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    commit_to_amend: &'a Commit,
    squash: bool,
) -> Result<(), Box<Error>> {
    let msg = if squash {
        format!("squash! {}", commit_to_amend.id())
    } else {
        format!("fixup! {}", commit_to_amend.id())
    };

    let sig = repo.signature()?;
    let mut idx = repo.index()?;
    let tree = repo.find_tree(idx.write_tree()?)?;
    let head_commit = head_branch.get().peel_to_commit()?;
    repo.commit(Some("HEAD"), &sig, &sig, &msg, &tree, &[&head_commit])?;
    Ok(())
}

fn select_commit_to_amend<'a>(
    repo: &'a Repository,
    upstream: Option<Branch<'a>>,
) -> Result<Commit<'a>, Box<Error>> {
    let mut walker = repo.revwalk()?;
    walker.push_head()?;
    let commits = if let Some(upstream) = upstream {
        let upstream_oid = upstream.get().target().expect("No upstream target");
        walker
            .flat_map(|r| r)
            .take_while(|rev| *rev != upstream_oid)
            .map(|rev| repo.find_commit(rev))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        walker
            .flat_map(|r| r)
            .take(15)
            .map(|rev| repo.find_commit(rev))
            .collect::<Result<Vec<_>, _>>()?
    };
    let rev_aliases = commits
        .iter()
        .map(|commit| {
            format!(
                "{} {}",
                &style(&commit.id().to_string()[0..10]).blue(),
                commit.summary().unwrap_or("no commit summary")
            )
        })
        .collect::<Vec<_>>();
    let commitmsgs = rev_aliases.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
    println!("Select a commit to amend:");
    let selected = Select::new().items(&commitmsgs).default(0).interact();
    Ok(repo.find_commit(commits[selected?].id())?)
}

fn print_diff(kind: Changes) -> Result<(), Box<Error>> {
    let mut args = vec!["diff", "--stat"];
    if kind == Changes::Staged {
        args.push("--cached");
    }
    let status = Command::new("git").args(&args).spawn()?.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err("git diff failed".into())
    }
}
