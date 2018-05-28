extern crate dialoguer;
extern crate git2;

use std::error::Error;
use std::process::Command;

use dialoguer::Select;
use git2::{Branch, Commit, DiffStatsFormat, Repository};

fn main() {
    if let Err(e) = run() {
        println!("Error running rebase: {}", e);
    }
}

fn run() -> Result<(), Box<Error>> {
    let repo = Repository::open(".")?;
    match repo.head() {
        Ok(head) => {
            let head_tree = head.peel_to_tree()?;
            let diffstat = repo.diff_tree_to_index(Some(&head_tree), None, None)?
                .stats()?;
            if diffstat.files_changed() == 0 {
                println!("Nothing staged, stage something");
                let workstat = repo.diff_tree_to_workdir(Some(&head_tree), None)?
                    .stats()?;
                println!(
                    "{}",
                    workstat
                        .to_buf(DiffStatsFormat::FULL, 80)?
                        .as_str()
                        .expect("Couldn't format diff as utf-8")
                );
                return Err("staging is unimplemented".into());
            } else {
                println!("Staged changes:");
                println!(
                    "{}",
                    diffstat
                        .to_buf(DiffStatsFormat::FULL, 80)?.as_str()
                        .expect("Couldn't format diff as utf-8")
                )
            }
            let head_branch = Branch::wrap(head);

            let commit_to_amend =
                select_commit_to_amend(&repo, head_branch.upstream().ok())?;
            println!("selected: {} {}", &commit_to_amend.id().to_string()[0..10], commit_to_amend.summary().unwrap_or(""));

            // create a fixup commit
            let msg = format!("fixup! {}", commit_to_amend.id());
            let sig = repo.signature()?;
            let mut idx = repo.index()?;
            let tree = repo.find_tree(idx.write_tree()?)?;
            let head_commit = head_branch.get().peel_to_commit()?;
            repo.commit(Some("HEAD"), &sig, &sig, &msg, &tree, &[&head_commit])?;

            // do the rebase
            let target_id = format!("{}~", commit_to_amend.id());
            Command::new("git")
                .args(&["rebase", "--interactive", "--autosquash", &target_id])
                .env("GIT_SEQUENCE_EDITOR", "true")
                .spawn()?
            .wait()?;
        }
        Err(e) => println!("head is not pointing at a valid branch: {}", e),
    };
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
                &commit.id().to_string()[0..10],
                commit.summary().unwrap_or("no commit summary")
            )
        })
        .collect::<Vec<_>>();
    let commitmsgs = rev_aliases.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
    println!("Select a commit:");
    let selected = Select::new().items(&commitmsgs).default(0).interact();
    Ok(repo.find_commit(commits[selected?].id())?)
}
