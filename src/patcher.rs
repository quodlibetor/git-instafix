//! mod patcher creates a patch/commit that represents the change to apply in the later rebase

mod diff_ui;

use anyhow::bail;
use dialoguer::Confirm;
use git2::Branch;
use git2::Commit;
use git2::Diff;
use git2::Repository;
use terminal_size::{terminal_size, Height};

use diff_ui::native_diff;
use diff_ui::print_diff_lines;
use diff_ui::print_diffstat;

/// Get a diff either from the index or the diff from the index to the working tree
pub(crate) fn create_diff<'a>(
    repo: &'a Repository,
    theme: &str,
    require_newline: bool,
) -> Result<Diff<'a>, anyhow::Error> {
    let head = repo.head()?;
    let head_tree = head.peel_to_tree()?;
    let staged_diff = repo.diff_tree_to_index(Some(&head_tree), None, None)?;
    let dirty_diff = repo.diff_index_to_workdir(None, None)?;
    let diffstat = staged_diff.stats()?;
    let diff = if diffstat.files_changed() == 0 {
        let dirty_workdir_stats = dirty_diff.stats()?;
        if dirty_workdir_stats.files_changed() > 0 {
            let Height(h) = terminal_size().map(|(_w, h)| h).unwrap_or(Height(24));
            let cutoff_height = (h - 5) as usize; // give some room for the prompt
            let total_change = dirty_workdir_stats.insertions() + dirty_workdir_stats.deletions();
            if total_change >= cutoff_height {
                print_diffstat("Unstaged", &dirty_diff)?;
            } else {
                let diff_lines = native_diff(&dirty_diff, theme)?;
                if diff_lines.len() >= cutoff_height {
                    print_diffstat("Unstaged", &dirty_diff)?;
                } else {
                    print_diff_lines(&diff_lines)?;
                }
            }
            if !Confirm::new()
                .with_prompt("Nothing staged, stage and commit everything?")
                .wait_for_newline(require_newline)
                .interact()?
            {
                bail!("");
            }
        } else {
            bail!("Nothing staged and no tracked files have any changes");
        }
        repo.apply(&dirty_diff, git2::ApplyLocation::Index, None)?;
        // the diff that we return knows whether it's from the index to the
        // workdir or the HEAD to the index, so now that we've created a new
        // commit we need a new diff.
        repo.diff_tree_to_index(Some(&head_tree), None, None)?
    } else {
        diff_ui::print_diffstat("Staged", &staged_diff)?;
        staged_diff
    };

    Ok(diff)
}

pub(crate) fn worktree_is_dirty(repo: &Repository) -> Result<bool, anyhow::Error> {
    let head = repo.head()?;
    let head_tree = head.peel_to_tree()?;
    let staged_diff = repo.diff_tree_to_index(Some(&head_tree), None, None)?;
    let dirty_diff = repo.diff_index_to_workdir(None, None)?;
    let diffstat = staged_diff.stats()?;
    let dirty_workdir_stats = dirty_diff.stats()?;
    Ok(diffstat.files_changed() > 0 || dirty_workdir_stats.files_changed() > 0)
}

/// Commit the current index as a fixup or squash commit
pub(crate) fn do_fixup_commit<'a>(
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
