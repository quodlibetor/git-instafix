use std::env;

use clap::Parser;

// Env vars that provide defaults for args
const MAX_COMMITS_VAR: &str = "GIT_INSTAFIX_MAX_COMMITS";
const UPSTREAM_VAR: &str = "GIT_INSTAFIX_UPSTREAM";
const REQUIRE_NEWLINE_VAR: &str = "GIT_INSTAFIX_REQUIRE_NEWLINE";
const THEME_VAR: &str = "GIT_INSTAFIX_THEME";

// Other defaults
pub(crate) const DEFAULT_UPSTREAM_BRANCHES: &[&str] = &["main", "master", "develop", "trunk"];
pub const DEFAULT_THEME: &str = "base16-ocean.dark";

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

/// Fully configured arguments after loading from env and gitconfig
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
    /// User requested info about themes
    pub help_themes: bool,
    /// Which theme to use
    pub theme: String,
}

/// Create a Config based on arguments and env vars
pub fn load_config_from_args_env_git() -> Config {
    let mut args = Args::parse();
    if env::args().next().unwrap().ends_with("squash") {
        args.squash = Some(true)
    }
    args_to_config_using_git_config(args).unwrap()
}

fn args_to_config_using_git_config(args: Args) -> Result<Config, anyhow::Error> {
    let mut cfg = git2::Config::open_default()?;
    let repo = git2::Repository::discover(".")?;
    cfg.add_file(&repo.path().join("config"), git2::ConfigLevel::Local, false)?;
    Ok(Config {
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
        help_themes: args.help_themes,
        theme: args.theme.unwrap_or_else(|| {
            cfg.get_string("instafix.theme")
                .unwrap_or_else(|_| DEFAULT_THEME.to_string())
        }),
    })
}
