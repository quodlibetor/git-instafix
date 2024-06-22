//! mod selector is responsible for tooling around selecting which commit to ammend

use std::collections::HashMap;

use anyhow::bail;
use console::style;
use dialoguer::Select;
use git2::BranchType;
use git2::Commit;
use git2::Object;
use git2::Oid;
use git2::{Branch, Repository};

use crate::format_ref;

pub(crate) fn select_commit_to_amend<'a>(
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

pub(crate) fn get_merge_base<'a>(
    repo: &'a Repository,
    head_branch: &'a Branch,
    upstream_name: Option<&str>,
) -> Result<Option<Object<'a>>, anyhow::Error> {
    let upstream = if let Some(explicit_upstream_name) = upstream_name {
        let branch = repo.find_branch(explicit_upstream_name, BranchType::Local)?;
        branch.into_reference().peel_to_commit()?
    } else if let Some(branch) = crate::config::DEFAULT_UPSTREAM_BRANCHES
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
