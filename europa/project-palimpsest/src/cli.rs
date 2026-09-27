//! CLI parsing. Both `--key value` and `--key=value` forms accepted;
//! positional path = repository.

use std::path::PathBuf;

pub struct Args {
    pub repo: Option<PathBuf>,
    pub demo: Option<DemoProfile>,
    pub dump: bool,
    pub width: u16,
    pub height: u16,
    pub mono: bool,
    pub ansi16: bool,
    pub view: Option<String>,
    pub commit: Option<String>,
    pub profile: bool,
    pub fullscreen: bool,
    pub limit: usize,
    pub seed_dir: Option<PathBuf>,
    pub file: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DemoProfile {
    Tiny,
    Medium,
    Large,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            repo: None,
            demo: None,
            dump: false,
            width: 120,
            height: 40,
            mono: false,
            ansi16: false,
            view: None,
            commit: None,
            profile: false,
            fullscreen: false,
            limit: 5000,
            seed_dir: None,
            file: None,
        }
    }
}

impl Args {
    pub fn parse() -> Self {
        let mut a = Self::default();
        let argv: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < argv.len() {
            let raw = argv[i].as_str();
            let (key, inline_val) = match raw.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (raw, None),
            };
            // values: inline (`--key=v`) or the next argv (`--key v`)
            let next = |i: &mut usize| -> Option<String> {
                if let Some(v) = inline_val {
                    return Some(v);
                }
                *i += 1;
                argv.get(*i).cloned()
            };
            match key {
                "--repo" | "-r" => {
                    if let Some(v) = next(&mut i) {
                        a.repo = Some(PathBuf::from(v));
                    }
                }
                "--demo" | "-d" => {
                    let v = next(&mut i);
                    a.demo = Some(match v.as_deref() {
                        Some("tiny") => DemoProfile::Tiny,
                        Some("large") | Some("big") => DemoProfile::Large,
                        _ => DemoProfile::Medium,
                    });
                }
                "--dump" => a.dump = true,
                "--width" | "-w" => {
                    if let Some(v) = next(&mut i) {
                        a.width = v.parse().unwrap_or(120);
                    }
                }
                "--height" => {
                    if let Some(v) = next(&mut i) {
                        a.height = v.parse().unwrap_or(40);
                    }
                }
                "--mono" | "-m" => a.mono = true,
                "--ansi16" => a.ansi16 = true,
                "--view" | "-v" => {
                    if let Some(v) = next(&mut i) {
                        a.view = Some(v);
                    }
                }
                "--commit" | "-c" => {
                    if let Some(v) = next(&mut i) {
                        a.commit = Some(v);
                    }
                }
                "--profile" | "-p" => a.profile = true,
                "--fullscreen" | "-F" => a.fullscreen = true,
                "--limit" | "-l" => {
                    if let Some(v) = next(&mut i) {
                        a.limit = v.parse().unwrap_or(5000);
                    }
                }
                "--file" | "-f" => {
                    if let Some(v) = next(&mut i) {
                        a.file = Some(v);
                    }
                }
                "--out-dir" => {
                    if let Some(v) = next(&mut i) {
                        a.seed_dir = Some(PathBuf::from(v));
                    }
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => {
                    if !other.starts_with('-') && a.repo.is_none() {
                        a.repo = Some(PathBuf::from(other));
                    }
                }
            }
            i += 1;
        }
        a
    }
}

fn print_help() {
    println!("PALIMPSEST — an interactive git repository time machine");
    println!();
    println!("Usage: palimpsest [OPTIONS] [PATH]");
    println!();
    println!("  PATH                      repository to open (default: discover upward from cwd)");
    println!("  --repo, -r <PATH>         explicit repository path");
    println!("  --demo[=tiny|medium|large] build + open a deterministic synthetic repository");
    println!("  --dump                    render one frame headlessly and exit (test/capture)");
    println!("  --width=<COLS>  -w        dump width  (default 120)");
    println!("  --height=<ROWS>           dump height (default 40)");
    println!("  --mono, -m                monochrome: shape-only grammar");
    println!("  --ansi16                  16-color palette");
    println!("  --view=atlas|strata|lens|provenance|health   initial view");
    println!("  --commit=<REV>            initial commit (rev or abbreviated oid)");
    println!("  --profile                 print load/render statistics to stderr");
    println!("  --fullscreen              alternate-screen mode (default: inline live-region)");
    println!("  --file=PATH               open strata/provenance for this path");
    println!("  --limit=N                 commit index bound (default 5000)");
    println!("  --out-dir=DIR             where demo fixtures are built (default $TMPDIR)");
    println!("  --help, -h                this help");
    println!();
    println!("In-app keys: ? help · 1..5 views · j/k step · ,. pan · +- zoom");
    println!("             Enter lens · / search · : command · c commit dossier · q quit");
}
