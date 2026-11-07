//! The command line.

use std::path::PathBuf;

use anyhow::{Result, bail};

/// What the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    /// The project bundle to open, created if missing.
    pub project: PathBuf,
    /// The sample rate for a new project.
    pub sample_rate: u32,
    /// Files to import as new tracks before opening.
    pub imports: Vec<PathBuf>,
    /// Export the mix to this file and exit instead of opening the window.
    pub export: Option<PathBuf>,
    /// Export the mono mix.
    pub mono: bool,
}

/// The usage text.
pub const USAGE: &str = "\
usage: debussy PROJECT.debussy [--rate HZ] [--import FILE]... [--export FILE [--mono]]

  Opens PROJECT.debussy, creating it at --rate (default 48000) if it does not exist.
  --import FILE   import WAV, FLAC, AAC or MP3 as a new track at the start (repeatable)
  --export FILE   render the whole project to FILE (.wav 24-bit, .flac 24-bit, .m4a AAC)
                  and exit without opening the window
  --mono          with --export, write the mono mix";

/// Parses the arguments after the program name.
///
/// # Errors
///
/// A message naming the bad argument.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut project = None;
    let mut sample_rate = 48_000;
    let mut imports = Vec::new();
    let mut export = None;
    let mut mono = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| match args.next() {
            Some(v) => Ok(v),
            None => bail!("{name} needs a value\n\n{USAGE}"),
        };
        match arg.as_str() {
            "--rate" => sample_rate = value("--rate")?.parse()?,
            "--import" => imports.push(PathBuf::from(value("--import")?)),
            "--export" => export = Some(PathBuf::from(value("--export")?)),
            "--mono" => mono = true,
            "-h" | "--help" => bail!("{USAGE}"),
            flag if flag.starts_with("--") => bail!("unknown option {flag}\n\n{USAGE}"),
            path if project.is_none() => project = Some(PathBuf::from(path)),
            extra => bail!("unexpected argument {extra}\n\n{USAGE}"),
        }
    }
    let Some(project) = project else {
        bail!("{USAGE}");
    };
    Ok(Args {
        project,
        sample_rate,
        imports,
        export,
        mono,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn test_parse_reads_every_option() -> Result<()> {
        let args = parse(strings(&[
            "Song.debussy",
            "--rate",
            "44100",
            "--import",
            "a.wav",
            "--import",
            "b.mp3",
            "--export",
            "mix.flac",
            "--mono",
        ]))?;
        assert_eq!(args.project, PathBuf::from("Song.debussy"));
        assert_eq!(args.sample_rate, 44_100);
        assert_eq!(
            args.imports,
            vec![PathBuf::from("a.wav"), PathBuf::from("b.mp3")]
        );
        assert_eq!(args.export, Some(PathBuf::from("mix.flac")));
        assert!(args.mono);
        Ok(())
    }

    #[test]
    fn test_parse_refuses_bad_input() {
        for bad in [
            &[][..],
            &["a", "b"],
            &["a", "--rate"],
            &["a", "--bogus"],
            &["a", "--rate", "x"],
        ] {
            assert!(parse(strings(bad)).is_err(), "{bad:?}");
        }
    }
}
