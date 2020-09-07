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
use std::process::Command;

use anyhow::{anyhow, bail};
use console::style;
use dialoguer::{Confirm, Select};
use git2::{Branch, Commit, Diff, Object, ObjectType, Oid, Rebase, Repository};
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

    /// Specify a commit to ammend by the subject line of the commit
    #[structopt(short = "P", long)]
    commit_message_pattern: Option<String>,
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
    if let Err(e) = run(args.squash, args.max_commits, args.commit_message_pattern) {
        // An empty message means don't display any error message
        let msg = e.to_string();
        if !msg.is_empty() {
            println!("Error: {:#}", e);
        }
        std::process::exit(1);
    }
}

fn run(
    _squash: bool,
    max_commits: usize,
    message_pattern: Option<String>,
) -> Result<(), anyhow::Error> {
    let repo = Repository::open(".")?;
    let diff = create_diff(&repo)?;
    let head = repo
        .head()
        .map_err(|e| anyhow!("HEAD is not pointing at a valid branch: {}", e))?;
    let head_branch = Branch::wrap(head);
    println!("head_branch: {:?}", head_branch.name().unwrap().unwrap());
    let upstream = get_upstream(&repo, &head_branch)?;
    let commit_to_amend = select_commit_to_amend(&repo, upstream, max_commits, &message_pattern)?;
    do_fixup_commit(&repo, &head_branch, &commit_to_amend, false)?;
    println!("selected: {}", disp(&commit_to_amend));
    let current_branch = Branch::wrap(repo.head()?);
    do_rebase(&repo, &current_branch, &commit_to_amend, &diff)?;

    Ok(())
}

fn do_rebase(
    repo: &Repository,
    branch: &Branch,
    commit_to_amend: &Commit,
    diff: &Diff,
) -> Result<(), anyhow::Error> {
    let first_parent = repo.find_annotated_commit(commit_parent(commit_to_amend)?.id())?;
    let branch_commit = repo.reference_to_annotated_commit(branch.get())?;
    let fixup_commit = branch.get().peel_to_commit()?;
    let fixup_message = fixup_commit.message();

    let rebase = &mut repo
        .rebase(Some(&branch_commit), Some(&first_parent), None, None)
        .map_err(|e| anyhow!("Error starting rebase: {}", e))?;
    match do_rebase_inner(repo, rebase, diff, fixup_message) {
        Ok(_) => {
            rebase.finish(None)?;
            Ok(())
        }
        Err(e) => {
            eprintln!("Aborting rebase, please apply it manualy via");
            eprintln!(
                "    git rebase --interactive --autosquash {}~",
                first_parent.id()
            );
            rebase.abort()?;
            Err(e)
        }
    }
}

fn do_rebase_inner(
    repo: &Repository,
    rebase: &mut Rebase,
    diff: &Diff,
    fixup_message: Option<&str>,
) -> Result<(), anyhow::Error> {
    let sig = repo.signature()?;

    match rebase.next() {
        Some(ref res) => {
            let op = res.as_ref().map_err(|e| anyhow!("No commit: {}", e))?;
            let target_commit = repo.find_commit(op.id())?;
            repo.apply(diff, git2::ApplyLocation::Both, None)?;
            let mut idx = repo.index()?;
            let oid = idx.write_tree()?;
            let tree = repo.find_tree(oid)?;

            // TODO: Support squash amends

            let rewrit_id = target_commit.amend(None, None, None, None, None, Some(&tree))?;
            repo.reset(
                &repo.find_object(rewrit_id, None)?,
                git2::ResetType::Soft,
                None,
            )?;

            rewrit_id
        }
        None => bail!("Unable to start rebase: no first step in rebase"),
    };

    while let Some(ref res) = rebase.next() {
        use git2::RebaseOperationType::*;

        let op = res.as_ref().map_err(|e| anyhow!("Err: {}", e))?;
        match op.kind() {
            Some(Pick) => {
                let commit = repo.find_commit(op.id())?;
                if commit.message() != fixup_message {
                    rebase.commit(None, &sig, None)?;
                }
            }
            Some(Fixup) | Some(Squash) | Some(Exec) | Some(Edit) | Some(Reword) => {
                // None of this should happen, we'd need to manually create the commits
                bail!("Unable to handle {:?} rebase operation", op.kind().unwrap())
            }
            None => {}
        }
    }

    Ok(())
}

fn commit_parent<'a>(commit: &'a Commit) -> Result<Commit<'a>, anyhow::Error> {
    match commit.parents().next() {
        Some(c) => Ok(c),
        None => bail!("Commit '{}' has no parents", disp(&commit)),
    }
}

/// Display a commit as "short_hash summary"
fn disp(commit: &Commit) -> String {
    format!(
        "{} {}",
        &commit.id().to_string()[0..10],
        commit.summary().unwrap_or("<no summary>"),
    )
}

fn get_upstream<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
) -> Result<Option<Object<'a>>, anyhow::Error> {
    let upstream = if let Ok(upstream_name) = env::var(UPSTREAM_VAR) {
        let branch = repo
            .branches(None)?
            .filter_map(|branch| branch.ok().map(|(b, _type)| b))
            .find(|b| {
                b.name()
                    .map(|n| n.expect("valid utf8 branchname") == &upstream_name)
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow!("cannot find branch with name {:?}", upstream_name))?;
        branch.into_reference().peel(ObjectType::Commit)?
    } else {
        if let Ok(upstream) = head_branch.upstream() {
            upstream.into_reference().peel(ObjectType::Commit)?
        } else {
            return Ok(None);
        }
    };

    let mb = repo.merge_base(
        head_branch
            .get()
            .target()
            .expect("all branches should ahve a target"),
        upstream.id(),
    )?;
    let commit = repo.find_object(mb, None).unwrap();

    Ok(Some(commit))
}

/// Get a diff either from the index or the diff from the index to the working tree
fn create_diff(repo: &Repository) -> Result<Diff, anyhow::Error> {
    let head = repo.head()?;
    let head_tree = head.peel_to_tree()?;
    let staged_diff = repo.diff_tree_to_index(Some(&head_tree), None, None)?;
    let diffstat = staged_diff.stats()?;
    let diff = if diffstat.files_changed() == 0 {
        let diff = repo.diff_index_to_workdir(None, None)?;
        let dirty_workdir_stats = diff.stats()?;
        if dirty_workdir_stats.files_changed() > 0 {
            print_diff(Changes::Unstaged)?;
            if !Confirm::new()
                .with_prompt("Nothing staged, stage and commit everything?")
                .interact()?
            {
                bail!("");
            }
        } else {
            bail!("Nothing staged and no tracked files have any changes");
        }
        repo.apply(&diff, git2::ApplyLocation::Index, None)?;
        diff
    } else {
        println!("Staged changes:");
        print_diff(Changes::Staged)?;
        staged_diff
    };

    Ok(diff)
}

/// Commit the current index as a fixup or squash commit
fn do_fixup_commit<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    commit_to_amend: &'a Commit,
    squash: bool,
) -> Result<(), anyhow::Error> {
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
    message_pattern: &Option<String>,
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
    if let Some(message_pattern) = message_pattern.as_ref() {
        commits
            .into_iter()
            .find(|commit| {
                commit
                    .summary()
                    .map(|s| s.contains(message_pattern))
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow::anyhow!("No commit contains the pattern in its summary"))
    } else {
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
            println!("Select a commit to amend (no upstream for HEAD):");
        } else {
            println!("Select a commit to amend:");
        }
        let selected = Select::new().items(&rev_aliases).default(0).interact();
        Ok(repo.find_commit(commits[selected?].id())?)
    }
}

fn format_ref(rf: &git2::Reference<'_>) -> Result<String, anyhow::Error> {
    let shorthand = rf.shorthand().unwrap_or("<unnamed>");
    let sha = rf.peel_to_commit()?.id().to_string();
    Ok(format!("{} ({})", shorthand, &sha[..10]))
}

fn print_diff(kind: Changes) -> Result<(), anyhow::Error> {
    let mut args = vec!["diff", "--stat"];
    if kind == Changes::Staged {
        args.push("--cached");
    }
    let status = Command::new("git").args(&args).spawn()?.wait()?;
    if status.success() {
        Ok(())
    } else {
        bail!("git diff failed")
    }
}
