use std::io::Write as _;

use anyhow::Context as _;
use git2::Diff;
use git2::DiffFormat;
use git2::DiffStatsFormat;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use termcolor::StandardStream;
use termcolor::{ColorChoice, WriteColor as _};

pub(crate) fn native_diff(diff: &Diff<'_>, theme: &str) -> Result<Vec<String>, anyhow::Error> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss.find_syntax_by_extension("patch").unwrap();
    let mut h = HighlightLines::new(
        syntax,
        ts.themes
            .get(theme)
            .unwrap_or_else(|| &ts.themes[crate::config::DEFAULT_THEME]),
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

pub(crate) fn print_diff_lines(diff_lines: &[String]) -> Result<(), anyhow::Error> {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    for line in diff_lines {
        write!(&mut stdout, "{}", line)?;
    }
    stdout.reset()?;
    writeln!(&mut stdout)?;
    Ok(())
}

pub(crate) fn print_diffstat(prefix: &str, diff: &Diff<'_>) -> Result<(), anyhow::Error> {
    let buf = diff.stats()?.to_buf(DiffStatsFormat::FULL, 80)?;
    let stat = std::str::from_utf8(&buf).context("converting diffstat to utf-8")?;
    println!("{prefix} changes:\n{stat}");

    Ok(())
}
