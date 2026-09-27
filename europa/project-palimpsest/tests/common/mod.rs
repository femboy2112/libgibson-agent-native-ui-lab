//! Shared test helpers: deterministic fixtures, headless rendering, ANSI
//! stripping, and plain-text frame flattening (what a user actually sees).

#![allow(dead_code)]

use std::path::PathBuf;

use gibson::context::{Context, RenderMode};
use palimpsest::app::App;
use palimpsest::fixture::{self, FixtureSpec};
use palimpsest::git::repo::Repo;
use palimpsest::theme::Palette;

/// Fixtures are shared with the `--demo` CLI (deterministic content), so a
/// single build serves both. A process lock inside `fixture::build` keeps
/// parallel test threads from racing on the same directory.
fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("palimpsest")
}

pub fn fixture_path(profile: &str) -> PathBuf {
    let dir = cache_dir();
    let spec = match profile {
        "tiny" => FixtureSpec::tiny(),
        "medium" => FixtureSpec::medium(),
        _ => FixtureSpec::large(),
    };
    fixture::build(&dir, &spec).expect("fixture build")
}

pub fn app_for(profile: &str) -> App {
    let path = fixture_path(profile);
    let repo = Repo::open(&path).expect("open");
    App::load(repo, Palette::color(), 5000).expect("app load")
}

/// Render one frame headlessly at the given geometry and return
/// `(raw_ansi, visible_text_rows)`.
pub fn render_frame(app: &mut App, w: u16, h: u16) -> (String, Vec<String>) {
    let mut ctx = Context::headless(RenderMode::Inline, w, h);
    ctx.set_color_depth(gibson::capability::ColorDepth::Ansi256);
    app.width = w;
    app.height = h;
    app.mark();
    let pal = app.palette;
    let tree = palimpsest::views::build_ui(app, &pal);
    ctx.set_root(tree);
    ctx.render().expect("render");
    let raw = ctx.take_output();
    (raw.clone(), flatten(&raw, w))
}

/// Replay a rendered byte stream onto a virtual character grid the way a
/// terminal would: honoring cursor positioning, line feeds and erase-to-EOL.
/// This tests *actual rendered output* — the visible truth, not model state.
pub fn flatten(raw: &str, width: u16) -> Vec<String> {
    let width = width as usize;
    let mut grid: Vec<Vec<char>> = vec![Vec::new(); 4096];
    let (mut x, mut y) = (0usize, 0usize);
    let mut chars = raw.chars().peekable();
    let mut in_csi = false;
    let mut csi_params = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                // escape sequence: CSI ... final-letter, OSC ... BEL/ST, or 2-char
                in_csi = false;
                csi_params.clear();
                if let Some(&n) = chars.peek() {
                    if n == '[' {
                        chars.next();
                        in_csi = true;
                    } else if n == ']' {
                        // OSC: swallow until BEL or ST
                        chars.next();
                        while let Some(o) = chars.next() {
                            if o == '\x07' {
                                break;
                            }
                            if o == '\x1b' {
                                if chars.peek() == Some(&'\\') {
                                    chars.next();
                                }
                                break;
                            }
                        }
                    } else {
                        chars.next();
                    }
                }
            }
            '\x07' => {}
            '\r' => x = 0,
            '\n' => {
                y += 1;
                x = 0;
            }
            c if in_csi => {
                if c.is_ascii_alphabetic() || c == '~' {
                    // apply the finished CSI
                    let params: Vec<i64> = csi_params
                        .split(';')
                        .filter(|p| !p.is_empty())
                        .map(|p| p.parse::<i64>().unwrap_or(0))
                        .collect();
                    match c {
                        'A' => {
                            y =
                                y.saturating_sub(
                                    params.first().copied().unwrap_or(1).max(1) as usize
                                )
                        }
                        'B' | 'e' => y += params.first().copied().unwrap_or(1).max(1) as usize,
                        'C' | 'a' => x += params.first().copied().unwrap_or(1).max(1) as usize,
                        'D' => {
                            x =
                                x.saturating_sub(
                                    params.first().copied().unwrap_or(1).max(1) as usize
                                )
                        }
                        'G' | '`' => {
                            // absolute column (1-indexed); 0 canonicalizes to 1
                            let col = params.first().copied().unwrap_or(1).max(1);
                            x = (col - 1) as usize;
                        }
                        'H' | 'f' => {
                            y = (params.first().copied().unwrap_or(1).max(1) - 1) as usize;
                            x = (params.get(1).copied().unwrap_or(1).max(1) - 1) as usize;
                        }
                        'd' => {
                            // absolute line (1-indexed)
                            let row = params.first().copied().unwrap_or(1).max(1);
                            y = (row - 1) as usize;
                        }
                        'J' => {
                            let mode = params.first().copied().unwrap_or(0);
                            if mode == 2 || mode == 0 {
                                if mode == 2 {
                                    for row in grid.iter_mut() {
                                        row.clear();
                                    }
                                    x = 0;
                                    y = 0;
                                } else {
                                    // erase from cursor to end of display
                                    if y < grid.len() {
                                        grid[y].truncate(x);
                                        for row in grid.iter_mut().skip(y + 1) {
                                            row.clear();
                                        }
                                    }
                                }
                            }
                        }
                        'K' => {
                            let mode = params.first().copied().unwrap_or(0);
                            if mode == 0 && y < 4096 {
                                grid[y].truncate(x);
                            } else if mode == 2 && y < 4096 {
                                grid[y].clear();
                                x = 0;
                            }
                        }
                        'L' => {
                            let n = params.first().copied().unwrap_or(1).max(1) as usize;
                            for _ in 0..n {
                                grid.insert(y, Vec::new());
                            }
                        }
                        _ => {}
                    }
                    in_csi = false;
                    csi_params.clear();
                } else {
                    csi_params.push(c);
                }
            }
            c => {
                if y < 4096 {
                    let row = &mut grid[y];
                    while row.len() < x {
                        row.push(' ');
                    }
                    if x < row.len() {
                        row[x] = c;
                    } else {
                        row.push(c);
                    }
                    x += 1;
                    if x >= width {
                        // DECAWM is disabled by the engine; clamp defensively
                        x = width.saturating_sub(1);
                    }
                }
            }
        }
    }
    grid.truncate(y + 2);
    grid.into_iter()
        .map(|row| {
            let s: String = row.into_iter().collect();
            s.trim_end().to_string()
        })
        .collect()
}

/// Strip all escape sequences (for substring assertions on raw output).
pub fn strip_ansi(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || c == '~' {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            } else {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn frame_text(rows: &[String]) -> String {
    rows.join("\n")
}
