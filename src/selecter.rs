//! mod selector is responsible for tooling around selecting which commit to ammend

use std::collections::HashMap;

use anyhow::{anyhow, bail};
use console::style;
use dialoguer::Select;
use git2::{Branch, BranchType, Commit, Oid, Reference, Repository};

use crate::config;
use crate::format_ref;

pub(crate) struct CommitSelection<'a> {
    pub commit: Commit<'a>,
    pub reference: Reference<'a>,
}

pub(crate) fn select_commit_to_amend<'a>(
    repo: &'a Repository,
    upstream: Option<CommitSelection>,
    max_commits: usize,
    message_pattern: Option<&str>,
) -> Result<Commit<'a>, anyhow::Error> {
    let mut walker = repo.revwalk()?;
    walker.push_head()?;
    let commits = if let Some(upstream) = upstream.as_ref() {
        let upstream_oid = upstream.commit.id();
        let commits = walker
            .flatten()
            .take_while(|rev| *rev != upstream_oid)
            .take(max_commits)
            .map(|rev| repo.find_commit(rev))
            .collect::<Result<Vec<_>, _>>()?;

        let head = repo.head()?;
        let current_branch_name = head
            .shorthand()
            .ok_or_else(|| anyhow!("HEAD's name is invalid utf-8"))?;
        if repo.head()?.peel_to_commit()?.id() == upstream.commit.id()
            && current_branch_name == upstream.reference.name().unwrap()
        {
            let upstream_setting = config::UPSTREAM_SETTING;
            bail!(
                "HEAD is already pointing at a common upstream branch\n\
            If you don't create branches for your work consider setting upstream to a remote ref:\n\
            \n    \
                git config {upstream_setting} origin/{current_branch_name}"
            )
        }
        commits
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
            upstream
                .map(|u| u.commit.id().to_string())
                .unwrap_or_else(|| "<no upstream>".to_string())
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

pub(crate) fn get_merge_base<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    upstream_name: Option<&str>,
) -> Result<Option<CommitSelection<'a>>, anyhow::Error> {
    let (upstream, branch) = if let Some(explicit_upstream_name) = upstream_name {
        let reference = repo.resolve_reference_from_short_name(explicit_upstream_name)?;
        let r2 = repo.resolve_reference_from_short_name(explicit_upstream_name)?;
        (reference.peel_to_commit()?, r2)
    } else if let Some(branch) = find_default_upstream_branch(repo) {
        (
            branch.into_reference().peel_to_commit()?,
            find_default_upstream_branch(repo).unwrap().into_reference(),
        )
    } else if let Ok(upstream) = head_branch.upstream() {
        (
            upstream.into_reference().peel_to_commit()?,
            head_branch.upstream().unwrap().into_reference(),
        )
    } else {
        return Ok(None);
    };

    let mb = repo.merge_base(
        head_branch
            .get()
            .target()
            .expect("all branches should have a target"),
        upstream.id(),
    )?;
    let commit = repo.find_object(mb, None).unwrap();

    Ok(Some(CommitSelection {
        commit: commit.peel_to_commit()?,
        reference: branch,
    }))
}

pub(crate) fn commit_id_and_summary(commits: &[Commit<'_>], idx: usize) -> String {
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

/// Check if any of the `config::DEFAULT_UPSTREAM_BRANCHES` exist in the repository
fn find_default_upstream_branch(repo: &Repository) -> Option<Branch> {
    crate::config::DEFAULT_UPSTREAM_BRANCHES
        .iter()
        .find_map(|b| repo.find_branch(b, BranchType::Local).ok())
}
