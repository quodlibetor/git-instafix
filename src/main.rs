extern crate dialoguer;
extern crate git2;

use std::error::Error;

use dialoguer::{Select, Confirmation};
use git2::{Branch, Commit, DiffStatsFormat, Repository};

fn main() {
    let repo = Repository::open(".").unwrap();

    match repo.head() {
        Ok(head) => {
            let head_tree = head.peel_to_tree().unwrap();
            let diffstat = repo.diff_tree_to_index(Some(&head_tree), None, None)
                .unwrap()
                .stats()
                .unwrap();
            if diffstat.files_changed() == 0 {
                println!("Nothing staged, stage something");
                let workstat = repo.diff_tree_to_workdir(Some(&head_tree), None)
                    .unwrap()
                    .stats()
                    .unwrap();
                println!(
                    "{}",
                    workstat
                        .to_buf(DiffStatsFormat::FULL, 80)
                        .unwrap()
                        .as_str()
                        .unwrap()
                );
                return;
            } else {
                println!("Staged changes:");
                println!(
                    "{}",
                    diffstat
                        .to_buf(DiffStatsFormat::FULL, 80)
                        .unwrap()
                        .as_str()
                        .unwrap()
                )
            }
            let head_branch = Branch::wrap(head);

            let commit = select_commit_to_amend(&repo, head_branch.upstream().ok()).unwrap();
            println!("selected: {}", commit.summary().unwrap());

            // create a fixup commit
            let msg = format!("fixup! {}", commit.id());
            let sig = repo.signature().unwrap();
            let mut idx = repo.index().unwrap();
            let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
            let head_commit = head_branch.get().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, &msg, &tree, &[&head_commit])
                .unwrap();

            // do the rebase
            let from_branch = repo.reference_to_annotated_commit(head_branch.get())
                .unwrap();
            let commit_parent = commit.parent(0).unwrap().id();
            let upstream = repo.find_annotated_commit(commit_parent).unwrap();
            let upstream2 = repo.find_annotated_commit(commit_parent).unwrap();
            //let to_branch = repo.find_annotated_commit(commit.parent(0).unwrap().id()).unwrap();
            let rebase = repo.rebase_init(Some(from_branch), Some(upstream), Some(upstream2), None)
                .unwrap();
            for event in rebase.operation_iter() {
                let event = event.unwrap();
                println!("rebase! {:?} {}", event.kind(), event.id().unwrap());
                let this_commit = repo.find_commit(event.id().unwrap()).unwrap();
                // if we're at the target we want to combine two commits
                // TODO: this... doesn't work. It just reports that there are 
                // merge conflicts
                let commit_tree = if this_commit.id() == commit.id() {
                    let parent = commit.parent(0).unwrap(); 
                    repo.reset(parent.as_object(), git2::ResetType::Soft, None).unwrap();
                    let upstream_tree = repo.find_tree(repo.index().unwrap().write_tree().unwrap()).unwrap();
                    println!("merging trees");
                    let mut merged_idx = repo.merge_trees(&parent.tree().unwrap(), &tree, &upstream_tree, None).unwrap();
                    println!("writing to repo {:?}\thas conflicts: {}", repo.path(), merged_idx.has_conflicts());
                    let oid = merged_idx.write_tree_to(&repo).unwrap();
                    println!("finding tree");
                    repo.find_tree(oid).unwrap()
                } else {
                    this_commit.tree().unwrap()
                };
                let parents = this_commit.parents().collect::<Vec<_>>();
                let parents_ref = parents.iter().collect::<Vec<&Commit>>();
                let prompt = format!("commit {}", this_commit.message().unwrap());
                Confirmation::new(&prompt).interact().unwrap();
                repo.commit(
                    Some("HEAD"),
                    &this_commit.author(),
                    &this_commit.committer(),
                    this_commit.message().unwrap(),
                    &commit_tree,
                    &parents_ref,
                ).unwrap();
            }
            rebase.finish(Some(&sig)).unwrap();
        }
        Err(e) => println!("head is not pointing at a valid branch: {}", e),
    };
}

fn select_commit_to_amend<'a>(
    repo: &'a Repository,
    upstream: Option<Branch<'a>>,
) -> Result<Commit<'a>, Box<Error>> {
    let mut walker = repo.revwalk()?;
    walker.push_head()?;
    let commits = if let Some(upstream) = upstream {
        let upstream_oid = upstream.get().target().unwrap();
        walker
            .flat_map(|r| r)
            .take_while(|rev| *rev != upstream_oid)
            .map(|rev| repo.find_commit(rev).unwrap())
            .collect::<Vec<_>>()
    } else {
        walker
            .flat_map(|r| r)
            .take(15)
            .map(|rev| repo.find_commit(rev).unwrap())
            .collect::<Vec<_>>()
    };
    let rev_aliases = commits
        .iter()
        .map(|commit| {
            format!(
                "{} {}",
                &commit.id().to_string()[0..10],
                commit.summary().unwrap()
            )
        })
        .collect::<Vec<_>>();
    let commitmsgs = rev_aliases.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
    println!("Select a commit:");
    let selected = Select::new().items(&commitmsgs).default(0).interact();
    Ok(repo.find_commit(commits[selected.unwrap()].id()).unwrap())
}
