use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    about = "Perform a rebase, and pull all the branches that were pointing at commits being rebased",
    max_term_width = 100,
    setting = structopt::clap::AppSettings::UnifiedHelpMessage,
    setting = structopt::clap::AppSettings::ColoredHelp,
)]
struct Args {
    /// The target ref
    onto: String,
}

fn main() {
    let args = Args::from_args();
    if let Err(e) = git_fixup::rebase_onto(&args.onto) {
        eprintln!("{:#}", e);
        std::process::exit(1);
    }
}
