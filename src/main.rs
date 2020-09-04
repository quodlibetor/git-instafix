// Copyright 2018 Brandon W Maister
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::process::Command;

use anyhow::bail;
use console::style;
use dialoguer::{Confirm, Select};
use git2::{Branch, Commit, Diff, Object, ObjectType, Oid, Repository};
use structopt::StructOpt;

const UPSTREAM_VAR: &str = "GIT_INSTAFIX_UPSTREAM";

#[derive(StructOpt, Debug)]
#[structopt(
    about = "Fix a commit in your history with your currently-staged changes",
    long_about = "Fix a commit in your history with your currently-staged changes

When run with no arguments this will:

  * If you have no staged changes, ask if you'd like to stage all changes
  * Print a `diff --stat` of your currently staged changes
  * Provide a list of commits to fixup or amend going back to:
      * The merge-base of HEAD and the environment var GIT_INSTAFIX_UPSTREAM
        (if it is set)
      * HEAD's upstream
  * Fixup your selected commit with the staged changes
",
    max_term_width = 100,
    setting = structopt::clap::AppSettings::UnifiedHelpMessage,
    setting = structopt::clap::AppSettings::ColoredHelp,
)]
struct Args {
    /// Use `squash!`: change the commit message that you amend
    #[structopt(short = "s", long = "squash")]
    squash: bool,
    /// The maximum number of commits to show when looking for your merge point
    #[structopt(short = "m", long = "max-commits", default_value = "15")]
    max_commits: usize,
}

#[derive(Eq, PartialEq, Debug)]
enum Changes {
    Staged,
    Unstaged,
}

fn main() {
    let mut args = Args::from_args();
    if env::args().next().unwrap().ends_with("squash") {
        args.squash = true
    }
    if let Err(e) = run(args.squash, args.max_commits) {
        // An empty message means don't display any error message
        let msg = e.to_string();
        if !msg.is_empty() {
            println!("Error: {}", e);
        }
    }
}

fn run(squash: bool, max_commits: usize) -> Result<(), Box<dyn Error>> {
    let repo = Repository::open(".")?;
    let head = repo
        .head()
        .map_err(|e| format!("HEAD is not pointing at a valid branch: {}", e))?;
    let head_tree = head.peel_to_tree()?;
    let head_branch = Branch::wrap(head);
    let diff = repo.diff_tree_to_index(Some(&head_tree), None, None)?;
    let upstream = get_upstream(&repo, &head_branch)?;
    let commit_to_amend =
        create_fixup_commit(&repo, &head_branch, upstream, &diff, squash, max_commits)?;
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
    Ok(())
}

fn get_upstream<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
) -> Result<Option<Object<'a>>, Box<dyn Error>> {
    let upstream = if let Ok(upstream_name) = env::var(UPSTREAM_VAR) {
        let branch = repo
            .branches(None)?
            .filter_map(|branch| branch.ok().map(|(b, _type)| b))
            .find(|b| {
                b.name()
                    .map(|n| n.expect("valid utf8 branchname") == &upstream_name)
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("cannot find branch with name {:?}", upstream_name))?;
        let result = Command::new("git")
            .args(&[
                "merge-base",
                head_branch.name().unwrap().unwrap(),
                branch.name().unwrap().unwrap(),
            ])
            .output()?
            .stdout;
        let oid = Oid::from_str(std::str::from_utf8(&result)?.trim())?;
        let commit = repo.find_object(oid, None).unwrap();

        commit
    } else {
        if let Ok(upstream) = head_branch.upstream() {
            upstream.into_reference().peel(ObjectType::Commit)?
        } else {
            return Ok(None);
        }
    };

    Ok(Some(upstream))
}

fn create_fixup_commit<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    upstream: Option<Object<'a>>,
    diff: &'a Diff,
    squash: bool,
    max_commits: usize,
) -> Result<Commit<'a>, Box<dyn Error>> {
    let diffstat = diff.stats()?;
    if diffstat.files_changed() == 0 {
        let dirty_workdir_stats = repo.diff_index_to_workdir(None, None)?.stats()?;
        if dirty_workdir_stats.files_changed() > 0 {
            print_diff(Changes::Unstaged)?;
            if !Confirm::new()
                .with_prompt("Nothing staged, stage and commit everything?")
                .interact()?
            {
                return Err("".into());
            }
        } else {
            return Err("Nothing staged and no tracked files have any changes".into());
        }
        let pathspecs: Vec<&str> = vec![];
        let mut idx = repo.index()?;
        idx.update_all(&pathspecs, None)?;
        idx.write()?;
    } else {
        println!("Staged changes:");
        print_diff(Changes::Staged)?;
    }
    let commit_to_amend = select_commit_to_amend(&repo, upstream, max_commits)?;
    do_fixup_commit(&repo, &head_branch, &commit_to_amend, squash)?;
    Ok(commit_to_amend)
}

fn do_fixup_commit<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    commit_to_amend: &'a Commit,
    squash: bool,
) -> Result<(), Box<dyn Error>> {
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
    upstream: Option<Object<'a>>,
    max_commits: usize,
) -> Result<Commit<'a>, anyhow::Error> {
    let mut walker = repo.revwalk()?;
    walker.push_head()?;
    let commits = if let Some(upstream) = upstream.as_ref() {
        let upstream_oid = upstream.id();
        walker
            .flat_map(|r| r)
            .take_while(|rev| *rev != upstream_oid)
            .take(max_commits)
            .map(|rev| repo.find_commit(rev))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        walker
            .flat_map(|r| r)
            .take(max_commits)
            .map(|rev| repo.find_commit(rev))
            .collect::<Result<Vec<_>, _>>()?
    };
    if commits.len() == 0 {
        bail!(
            "No commits between {} and {:?}",
            format_ref(&repo.head()?)?,
            upstream.map(|u| u.id()).unwrap()
        );
    }
    let branches: HashMap<Oid, String> = repo
        .branches(None)?
        .filter_map(|b| {
            b.ok().and_then(|(b, _type)| {
                let name: Option<String> = b.name().ok().and_then(|n| n.map(|n| n.to_owned()));
                let oid = b.into_reference().resolve().ok().and_then(|r| r.target());
                name.and_then(|name| oid.map(|oid| (oid, name)))
            })
        })
        .collect();
    let rev_aliases = commits
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let bname = if i > 0 {
                branches
                    .get(&commit.id())
                    .map(|n| format!("({}) ", n))
                    .unwrap_or_else(String::new)
            } else {
                String::new()
            };
            format!(
                "{} {}{}",
                &style(&commit.id().to_string()[0..10]).blue(),
                style(bname).green(),
                commit.summary().unwrap_or("no commit summary")
            )
        })
        .collect::<Vec<_>>();
    if upstream.is_none() {
        eprintln!("Select a commit to amend (no upstream for HEAD):");
    } else {
        eprintln!("Select a commit to amend:");
    }
    let selected = Select::new().items(&rev_aliases).default(0).interact();
    Ok(repo.find_commit(commits[selected?].id())?)
}

fn format_ref(rf: &git2::Reference<'_>) -> Result<String, anyhow::Error> {
    let shorthand = rf.shorthand().unwrap_or("<unnamed>");
    let sha = rf.peel_to_commit()?.id().to_string();
    Ok(format!("{} ({})", shorthand, &sha[..10]))
}

fn print_diff(kind: Changes) -> Result<(), Box<dyn Error>> {
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
