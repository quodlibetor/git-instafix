use std::collections::HashMap;

use anyhow::Context as _;
use anyhow::{anyhow, bail};
use git2::AnnotatedCommit;
use git2::Branch;
use git2::Commit;
use git2::Diff;
use git2::Oid;
use git2::{Rebase, Repository};

use crate::commit_display;

pub(crate) fn do_rebase(
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
        .context("starting rebase")?;

    let mut branches = RepoBranches::for_repo(repo)?;

    if let Err(e) = apply_diff_in_rebase(repo, rebase, diff, &mut branches) {
        print_help_and_abort_rebase(rebase, &first_parent).context("aborting rebase")?;
        return Err(e);
    }

    match do_rebase_inner(repo, rebase, fixup_message, branches) {
        Ok(_) => {
            rebase.finish(None)?;
            Ok(())
        }
        Err(e) => {
            print_help_and_abort_rebase(rebase, &first_parent).context("aborting rebase")?;
            Err(e)
        }
    }
}

pub(crate) fn print_help_and_abort_rebase(
    rebase: &mut Rebase,
    first_parent: &AnnotatedCommit,
) -> Result<(), git2::Error> {
    eprintln!("Aborting rebase, your changes are in the head commit.");
    eprintln!("You can apply it manually via:");
    eprintln!(
        "    git rebase --interactive --autosquash {}~",
        first_parent.id()
    );
    rebase.abort()?;
    Ok(())
}

pub(crate) fn apply_diff_in_rebase(
    repo: &Repository,
    rebase: &mut Rebase,
    diff: &Diff,
    branches: &mut RepoBranches,
) -> Result<(), anyhow::Error> {
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
            let rewrit_object = repo.find_object(rewrit_id, None)?;
            let rewrit_commit_id = repo.find_commit(rewrit_object.id())?.id();
            let retargeted =
                branches.retarget_branches(target_commit.id(), rewrit_commit_id, rebase)?;
            for b in retargeted {
                println!("{}", b);
            }

            repo.reset(&rewrit_object, git2::ResetType::Soft, None)?;
        }
        None => bail!("Unable to start rebase: no first step in rebase"),
    };
    Ok(())
}

/// Do a rebase, pulling all intermediate branches along the way
pub(crate) fn do_rebase_inner(
    repo: &Repository,
    rebase: &mut Rebase,
    fixup_message: Option<&str>,
    mut branches: RepoBranches,
) -> Result<(), anyhow::Error> {
    let sig = repo.signature()?;

    while let Some(ref res) = rebase.next() {
        use git2::RebaseOperationType::*;

        let op = res.as_ref().map_err(|e| anyhow!("Err: {}", e))?;
        match op.kind() {
            Some(Pick) => {
                let commit = repo.find_commit(op.id())?;
                let message = commit.message();
                if message.is_some() && message != fixup_message {
                    let new_id = rebase.commit(None, &sig, None)?;
                    let retargeted = branches.retarget_branches(commit.id(), new_id, rebase)?;
                    for b in retargeted {
                        println!("{}", b);
                    }
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

pub(crate) struct RepoBranches<'a>(HashMap<Oid, Vec<Branch<'a>>>);

pub(crate) struct RetargetedBranch {
    pub(crate) name: String,
    pub(crate) from: Oid,
    pub(crate) to: Oid,
}

impl std::fmt::Display for RetargetedBranch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let from = &self.from.to_string()[..15];
        let to = &self.to.to_string()[..15];
        let name = &self.name;
        f.write_fmt(format_args!("updated branch {name}: {from} -> {to}"))
    }
}

impl<'a> RepoBranches<'a> {
    pub(crate) fn for_repo(repo: &'a Repository) -> Result<RepoBranches<'a>, anyhow::Error> {
        let mut branches: HashMap<Oid, Vec<Branch>> = HashMap::new();
        for (branch, _type) in repo.branches(Some(git2::BranchType::Local))?.flatten() {
            let oid = branch.get().peel_to_commit()?.id();
            branches.entry(oid).or_default().push(branch);
        }
        Ok(RepoBranches(branches))
    }

    /// Move branches whos commits have moved
    pub(crate) fn retarget_branches(
        &mut self,
        original_commit: Oid,
        target_commit: Oid,
        rebase: &mut Rebase<'_>,
    ) -> Result<Vec<RetargetedBranch>, anyhow::Error> {
        let mut retargeted = vec![];
        if let Some(branches) = self.0.get_mut(&original_commit) {
            // Don't retarget the last branch, rebase.finish does that for us
            if rebase.operation_current() != Some(rebase.len() - 1) {
                for branch in branches.iter_mut() {
                    retargeted.push(RetargetedBranch {
                        name: branch
                            .name()
                            .context("getting a branch name")?
                            .ok_or(anyhow!("branch should have a name"))?
                            .to_owned(),
                        from: original_commit,
                        to: target_commit,
                    });
                    branch
                        .get_mut()
                        .set_target(target_commit, "git-instafix retarget historical branch")?;
                }
            }
        }
        Ok(retargeted)
    }
}

pub(crate) fn commit_parent<'a>(commit: &'a Commit) -> Result<Commit<'a>, anyhow::Error> {
    match commit.parents().next() {
        Some(c) => Ok(c),
        None => bail!("Commit '{}' has no parents", commit_display(commit)),
    }
}
