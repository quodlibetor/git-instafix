mod config;
mod patcher;
mod rebaser;
mod selecter;

use anyhow::Context;
use git2::{Branch, Commit, Repository};
use syntect::highlighting::ThemeSet;

pub use config::load_config_from_args_env_git;

pub fn instafix(c: config::Config) -> Result<(), anyhow::Error> {
    let repo = Repository::open_from_env().context("opening repo")?;
    let diff = patcher::create_diff(&repo, &c.theme, c.require_newline).context("creating diff")?;
    let head = repo.head().context("finding head commit")?;
    let head_branch = Branch::wrap(head);
    let upstream =
        selecter::get_merge_base(&repo, &head_branch, c.default_upstream_branch.as_deref())
            .context("creating merge base")?;
    let commit_to_amend = selecter::select_commit_to_amend(
        &repo,
        upstream,
        c.max_commits,
        c.commit_message_pattern.as_deref(),
    )
    .context("selecting commit to amend")?;
    eprintln!("Selected {}", commit_display(&commit_to_amend));
    patcher::do_fixup_commit(&repo, &head_branch, &commit_to_amend, c.squash)
        .context("doing fixup commit")?;
    let needs_stash = patcher::worktree_is_dirty(&repo)?;
    if needs_stash {
        // TODO: is it reasonable to create a new repo to work around lifetime issues?
        let mut repo = Repository::open_from_env()?;
        let sig = repo.signature()?.clone();
        repo.stash_save(&sig, "git-instafix stashing changes", None)?;
    }
    let current_branch = Branch::wrap(repo.head()?);
    rebaser::do_rebase(&repo, &current_branch, &commit_to_amend, &diff)?;
    if needs_stash {
        let mut repo = Repository::open(".")?;
        repo.stash_pop(0, None)?;
    }

    Ok(())
}

/// Display a commit as "short_hash summary"
fn commit_display(commit: &Commit) -> String {
    format!(
        "{} {}",
        &commit.id().to_string()[0..10],
        commit.summary().unwrap_or("<no summary>"),
    )
}

fn format_ref(rf: &git2::Reference<'_>) -> Result<String, anyhow::Error> {
    let shorthand = rf.shorthand().unwrap_or("<unnamed>");
    let sha = rf.peel_to_commit()?.id().to_string();
    Ok(format!("{} ({})", shorthand, &sha[..10]))
}

/// A vec of all built-in theme names
pub fn print_themes() {
    println!("Available themes:");
    for theme in ThemeSet::load_defaults().themes.keys() {
        println!("  {}", theme);
    }
}
