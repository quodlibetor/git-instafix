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

use std::env;

use clap::Parser;
use git_instafix::DEFAULT_THEME;

const MAX_COMMITS_VAR: &str = "GIT_INSTAFIX_MAX_COMMITS";
const UPSTREAM_VAR: &str = "GIT_INSTAFIX_UPSTREAM";
const REQUIRE_NEWLINE_VAR: &str = "GIT_INSTAFIX_REQUIRE_NEWLINE";
const THEME_VAR: &str = "GIT_INSTAFIX_THEME";

#[derive(Parser, Debug)]
#[clap(
    version,
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
    max_term_width = 100
)]
struct Args {
    /// Change the commit message that you amend, instead of using the original commit message
    #[clap(short = 's', long, hide = true)]
    squash: Option<bool>,
    /// The maximum number of commits to show when looking for your merge point
    ///
    /// [gitconfig: instafix.max-commits]
    #[clap(short = 'm', long = "max-commits", env = MAX_COMMITS_VAR)]
    max_commits: Option<usize>,

    /// Specify a commit to ammend by the subject line of the commit
    #[clap(short = 'P', long)]
    commit_message_pattern: Option<String>,

    /// The branch to not go past when looking for your merge point
    ///
    /// [gitconfig: instafix.default-upstream-branch]
    #[clap(short = 'u', long, env = UPSTREAM_VAR)]
    default_upstream_branch: Option<String>,

    /// Require a newline when confirming y/n questions
    ///
    /// [gitconfig: instafix.require-newline]
    #[clap(long, env = REQUIRE_NEWLINE_VAR)]
    require_newline: Option<bool>,

    /// Show the possible color themes for output
    #[clap(long)]
    help_themes: bool,

    /// Use this theme
    #[clap(long, env = THEME_VAR)]
    theme: Option<String>,
}

fn main() {
    let mut args = Args::parse();
    if env::args().next().unwrap().ends_with("squash") {
        args.squash = Some(true)
    }
    if args.help_themes {
        git_instafix::print_themes();
        return;
    }
    let config = args_to_config_using_git_config(args).unwrap();
    if let Err(e) = git_instafix::instafix(config) {
        // An empty message means don't display any error message
        let msg = e.to_string();
        if !msg.is_empty() {
            if env::var("RUST_BACKTRACE").as_deref() == Ok("1") {
                println!("Error: {:?}", e);
            } else {
                println!("Error: {:#}", e);
            }
        }
        std::process::exit(1);
    }
}

fn args_to_config_using_git_config(args: Args) -> Result<git_instafix::Config, anyhow::Error> {
    let mut cfg = git2::Config::open_default()?;
    let repo = git2::Repository::discover(".")?;
    cfg.add_file(&repo.path().join("config"), git2::ConfigLevel::Local, false)?;
    Ok(git_instafix::Config {
        squash: args
            .squash
            .unwrap_or_else(|| cfg.get_bool("instafix.squash").unwrap_or(false)),
        max_commits: args
            .max_commits
            .unwrap_or_else(|| cfg.get_i32("instafix.max-commits").unwrap_or(15) as usize),
        commit_message_pattern: args.commit_message_pattern,
        default_upstream_branch: args
            .default_upstream_branch
            .or_else(|| cfg.get_string("instafix.default-upstream-branch").ok()),
        require_newline: args
            .require_newline
            .unwrap_or_else(|| cfg.get_bool("instafix.require-newline").unwrap_or(false)),
        theme: args.theme.unwrap_or_else(|| {
            cfg.get_string("instafix.theme")
                .unwrap_or_else(|_| DEFAULT_THEME.to_string())
        }),
    })
}
