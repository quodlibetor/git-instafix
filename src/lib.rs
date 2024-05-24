use std::collections::HashMap;
use std::io::Write as _;

use anyhow::{anyhow, bail, Context};
use console::style;
use dialoguer::{Confirm, Select};
use git2::{
    AnnotatedCommit, Branch, BranchType, Commit, Diff, DiffFormat, DiffStatsFormat, Object, Oid,
    Rebase, Repository,
};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use termcolor::{ColorChoice, StandardStream, WriteColor as _};
use terminal_size::{terminal_size, Height};

const DEFAULT_UPSTREAM_BRANCHES: &[&str] = &["main", "master", "develop", "trunk"];
pub const DEFAULT_THEME: &str = "base16-ocean.dark";

pub struct Config {
    /// Change the commit message that you amend, instead of using the original commit message
    pub squash: bool,
    /// The maximum number of commits to show when looking for your merge point
    pub max_commits: usize,
    /// Specify a commit to ammend by the subject line of the commit
    pub commit_message_pattern: Option<String>,
    pub default_upstream_branch: Option<String>,
    /// Require a newline when confirming y/n questions
    pub require_newline: bool,
    /// Which theme to use
    pub theme: String,
}

pub fn instafix(c: Config) -> Result<(), anyhow::Error> {
    let repo = Repository::open(".").context("opening repo")?;
    let diff = create_diff(&repo, &c.theme, c.require_newline).context("creating diff")?;
    let head = repo.head().context("finding head commit")?;
    let head_branch = Branch::wrap(head);
    let upstream = get_merge_base(&repo, &head_branch, c.default_upstream_branch.as_deref())
        .context("creating merge base")?;
    let commit_to_amend = select_commit_to_amend(
        &repo,
        upstream,
        c.max_commits,
        c.commit_message_pattern.as_deref(),
    )
    .context("selecting commit to amend")?;
    eprintln!("Selected {}", disp(&commit_to_amend));
    do_fixup_commit(&repo, &head_branch, &commit_to_amend, c.squash)
        .context("doing fixup commit")?;
    let needs_stash = worktree_is_dirty(&repo)?;
    if needs_stash {
        // TODO: is it reasonable to create a new repo to work around lifetime issues?
        let mut repo = Repository::open(".")?;
        let sig = repo.signature()?.clone();
        repo.stash_save(&sig, "git-instafix stashing changes", None)?;
    }
    let current_branch = Branch::wrap(repo.head()?);
    do_rebase(&repo, &current_branch, &commit_to_amend, &diff)?;
    if needs_stash {
        let mut repo = Repository::open(".")?;
        repo.stash_pop(0, None)?;
    }

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

fn print_help_and_abort_rebase(
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

fn apply_diff_in_rebase(
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
            branches.retarget_branches(target_commit.id(), rewrit_commit_id, rebase)?;

            repo.reset(&rewrit_object, git2::ResetType::Soft, None)?;
        }
        None => bail!("Unable to start rebase: no first step in rebase"),
    };
    Ok(())
}

/// Do a rebase, pulling all intermediate branches along the way
fn do_rebase_inner(
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
                    branches.retarget_branches(commit.id(), new_id, rebase)?;
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

struct RepoBranches<'a>(HashMap<Oid, Vec<Branch<'a>>>);

impl<'a> RepoBranches<'a> {
    fn for_repo(repo: &'a Repository) -> Result<RepoBranches<'a>, anyhow::Error> {
        let mut branches: HashMap<Oid, Vec<Branch>> = HashMap::new();
        for (branch, _type) in repo.branches(Some(git2::BranchType::Local))?.flatten() {
            let oid = branch.get().peel_to_commit()?.id();
            branches.entry(oid).or_default().push(branch);
        }
        Ok(RepoBranches(branches))
    }

    /// Move branches whos commits have moved
    fn retarget_branches(
        &mut self,
        original_commit: Oid,
        target_commit: Oid,
        rebase: &mut Rebase<'_>,
    ) -> Result<(), anyhow::Error> {
        if let Some(branches) = self.0.get_mut(&original_commit) {
            // Don't retarget the last branch, rebase.finish does that for us
            if rebase.operation_current() != Some(rebase.len() - 1) {
                for branch in branches.iter_mut() {
                    branch
                        .get_mut()
                        .set_target(target_commit, "git-instafix retarget historical branch")?;
                }
            }
        }
        Ok(())
    }
}

fn commit_parent<'a>(commit: &'a Commit) -> Result<Commit<'a>, anyhow::Error> {
    match commit.parents().next() {
        Some(c) => Ok(c),
        None => bail!("Commit '{}' has no parents", disp(commit)),
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

fn get_merge_base<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    upstream_name: Option<&str>,
) -> Result<Option<Object<'a>>, anyhow::Error> {
    let upstream = if let Some(explicit_upstream_name) = upstream_name {
        let branch = repo.find_branch(explicit_upstream_name, BranchType::Local)?;
        branch.into_reference().peel_to_commit()?
    } else if let Some(branch) = DEFAULT_UPSTREAM_BRANCHES
        .iter()
        .find_map(|b| repo.find_branch(b, BranchType::Local).ok())
    {
        branch.into_reference().peel_to_commit()?
    } else if let Ok(upstream) = head_branch.upstream() {
        upstream.into_reference().peel_to_commit()?
    } else {
        return Ok(None);
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
fn create_diff<'a>(
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
        print_diffstat("Staged", &staged_diff)?;
        staged_diff
    };

    Ok(diff)
}

fn worktree_is_dirty(repo: &Repository) -> Result<bool, anyhow::Error> {
    let head = repo.head()?;
    let head_tree = head.peel_to_tree()?;
    let staged_diff = repo.diff_tree_to_index(Some(&head_tree), None, None)?;
    let dirty_diff = repo.diff_index_to_workdir(None, None)?;
    let diffstat = staged_diff.stats()?;
    let dirty_workdir_stats = dirty_diff.stats()?;
    Ok(diffstat.files_changed() > 0 || dirty_workdir_stats.files_changed() > 0)
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
    message_pattern: Option<&str>,
) -> Result<Commit<'a>, anyhow::Error> {
    let mut walker = repo.revwalk()?;
    walker.push_head()?;
    let commits = if let Some(upstream) = upstream.as_ref() {
        let upstream_oid = upstream.id();
        walker
            .flatten()
            .take_while(|rev| *rev != upstream_oid)
            .take(max_commits)
            .map(|rev| repo.find_commit(rev))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        walker
            .flatten()
            .take(max_commits)
            .map(|rev| repo.find_commit(rev))
            .collect::<Result<Vec<_>, _>>()?
    };
    if commits.is_empty() {
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
        let first = commit_id_and_summary(&commits, commits.len() - 1);
        let last = commit_id_and_summary(&commits, 0);
        commits
            .into_iter()
            .find(|commit| {
                commit
                    .summary()
                    .map(|s| s.contains(message_pattern))
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No commit contains the pattern in its summary between {}..{}",
                    first,
                    last
                )
            })
    } else {
        let rev_aliases = commits
            .iter()
            .enumerate()
            .map(|(i, commit)| {
                let bname = if i > 0 {
                    branches
                        .get(&commit.id())
                        .map(|n| format!("({}) ", n))
                        .unwrap_or_default()
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

fn commit_id_and_summary(commits: &[Commit<'_>], idx: usize) -> String {
    let first = commits
        .get(idx)
        .map(|c| {
            format!(
                "{} ({})",
                &c.id().to_string()[..10],
                c.summary().unwrap_or("<unknown>")
            )
        })
        .unwrap_or_else(|| "<unknown>".into());
    first
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

// diff helpers

fn native_diff(diff: &Diff<'_>, theme: &str) -> Result<Vec<String>, anyhow::Error> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss.find_syntax_by_extension("patch").unwrap();
    let mut h = HighlightLines::new(
        syntax,
        ts.themes
            .get(theme)
            .unwrap_or_else(|| &ts.themes[DEFAULT_THEME]),
    );

    let mut inner_err = None;
    let mut diff_lines = Vec::new();

    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let content = std::str::from_utf8(line.content()).unwrap();
        let origin = line.origin();
        match origin {
            '+' | '-' | ' ' => {
                let diff_line = format!("{origin}{content}");
                let ranges = match h.highlight_line(&diff_line, &ss) {
                    Ok(ranges) => ranges,
                    Err(err) => {
                        inner_err = Some(err);
                        return false;
                    }
                };
                let escaped = as_24_bit_terminal_escaped(&ranges[..], true);
                diff_lines.push(escaped);
            }
            _ => {
                let ranges = match h.highlight_line(content, &ss) {
                    Ok(ranges) => ranges,
                    Err(err) => {
                        inner_err = Some(err);
                        return false;
                    }
                };
                let escaped = as_24_bit_terminal_escaped(&ranges[..], true);
                diff_lines.push(escaped);
            }
        }
        true
    })?;

    if let Some(err) = inner_err {
        Err(err.into())
    } else {
        Ok(diff_lines)
    }
}

fn print_diff_lines(diff_lines: &[String]) -> Result<(), anyhow::Error> {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    for line in diff_lines {
        write!(&mut stdout, "{}", line)?;
    }
    stdout.reset()?;
    writeln!(&mut stdout)?;
    Ok(())
}

fn print_diffstat(prefix: &str, diff: &Diff<'_>) -> Result<(), anyhow::Error> {
    let buf = diff.stats()?.to_buf(DiffStatsFormat::FULL, 80)?;
    let stat = std::str::from_utf8(&buf).context("converting diffstat to utf-8")?;
    println!("{prefix} changes:\n{stat}");

    Ok(())
}
