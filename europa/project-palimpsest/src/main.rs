//! PALIMPSEST — an interactive git repository time machine / code archaeology
//! workstation, built against LibGibson v0.2.0.
//!
//! Default embedding is INLINE: the live region sits at the bottom of the
//! terminal and finalized artifacts are committed into native scrollback
//! above it — the immutable-history/live-region architecture the library is
//! built around. `--fullscreen` opts into the alternate screen instead.

use palimpsest::app;
use palimpsest::fixture;
use palimpsest::git;
use palimpsest::views;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gibson::capability::ColorDepth;
use gibson::cell::{Line as RLine, RichText};
use gibson::context::{Context, RenderMode};
use gibson::input::Event;

use palimpsest::app::{App, View};
use palimpsest::cli::{Args, DemoProfile};
use palimpsest::fixture::FixtureSpec;
use palimpsest::theme::Palette;

fn main() {
    let args = Args::parse();

    // Resolve the repository (open, discover, or build a deterministic fixture).
    let repo_path = match prepare_repo(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("palimpsest: {}", e);
            std::process::exit(1);
        }
    };

    let repo = match git::Repo::open(&repo_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("palimpsest: {}", e);
            std::process::exit(1);
        }
    };

    let palette = if args.mono {
        Palette::mono()
    } else if args.ansi16 {
        Palette::ansi16()
    } else {
        Palette::color()
    };

    let t0 = Instant::now();
    let app_result = App::load(repo, palette, args.limit);
    let mut app = match app_result {
        Ok(a) => a,
        Err(e) => {
            eprintln!("palimpsest: {}", e);
            std::process::exit(1);
        }
    };
    let load_more = t0.elapsed();

    // Initial view / commit wiring.
    if let Some(v) = &args.view {
        if let Some(view) = view_from_str(v) {
            if view == View::Lens {
                app.ensure_lens();
            }
            if view == View::Health {
                app.ensure_metrics();
            }
            app.view = view;
        }
    }
    if let Some(rev) = &args.commit {
        match app.repo.resolve(rev) {
            Ok(oid) => {
                if !app.select_oid(&oid) {
                    eprintln!(
                        "palimpsest: {} resolves but is outside the indexed window",
                        rev
                    );
                }
            }
            Err(e) => eprintln!("palimpsest: {}", e),
        }
    }
    if let Some(file) = &args.file {
        if app.view == View::Provenance {
            app.ensure_prov(file);
        } else {
            app.ensure_strata(file);
            if app.view == View::Atlas {
                app.view = View::Strata;
            }
        }
    }
    if args.dump {
        run_dump(&mut app, &args);
    } else {
        run_interactive(&mut app, &args, load_more);
    }
}

fn view_from_str(s: &str) -> Option<View> {
    match s.to_lowercase().as_str() {
        "atlas" | "1" => Some(View::Atlas),
        "strata" | "file" | "2" => Some(View::Strata),
        "lens" | "diff" | "3" => Some(View::Lens),
        "provenance" | "blame" | "prov" | "4" => Some(View::Provenance),
        "health" | "metrics" | "5" => Some(View::Health),
        _ => None,
    }
}

/// Open `--repo PATH`, discover from cwd, or build the requested fixture.
fn prepare_repo(args: &Args) -> Result<PathBuf, String> {
    if let Some(profile) = args.demo {
        let spec = match profile {
            DemoProfile::Tiny => FixtureSpec::tiny(),
            DemoProfile::Medium => FixtureSpec::medium(),
            DemoProfile::Large => FixtureSpec::large(),
        };
        let base = args
            .seed_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("palimpsest"));
        let t = Instant::now();
        let path = fixture::build(&base, &spec)?;
        eprintln!(
            "fixture `{}` built at {} in {:?}",
            spec.profile,
            path.display(),
            t.elapsed()
        );
        return Ok(path);
    }
    if let Some(p) = &args.repo {
        return Ok(p.clone());
    }
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {}", e))?;
    discover_repo(&cwd)
}

/// Walk upward from `start` looking for a .git directory; also accept a
/// .git file (worktrees).
fn discover_repo(start: &Path) -> Result<PathBuf, String> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    Err(format!(
        "no repository found at or above {} — pass a PATH or use --demo",
        start.display()
    ))
}

// ---------------------------------------------------------------------------
// Interactive loop (inline live-region by default)
// ---------------------------------------------------------------------------

fn run_interactive(app: &mut App, args: &Args, _load_more: Duration) {
    let mode = if args.fullscreen {
        RenderMode::Fullscreen
    } else {
        RenderMode::Inline
    };
    let mut ctx = match Context::new(mode) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("palimpsest: terminal init failed: {}", e);
            std::process::exit(1);
        }
    };
    // LibGibson suppresses live frames when stdout is not a TTY (correct for
    // CI), but an interactive launch must not silently idle: explain and exit.
    if !ctx.session.is_tty {
        eprintln!(
            "palimpsest: stdout is not a terminal — the live UI needs a TTY.\n  \
             For headless capture use: palimpsest --dump --width=120 --height=40 [--view=...]"
        );
        let _ = ctx.restore();
        std::process::exit(2);
    }
    let depth = if args.mono || args.ansi16 {
        ColorDepth::Ansi16
    } else {
        ColorDepth::Ansi256
    };
    ctx.set_color_depth(depth);

    app.width = ctx.session.terminal_size().0.max(20);
    app.height = ctx.session.terminal_size().1.max(6);
    app.mark();

    // Opening banner in scrollback: the instrument introduces itself.
    {
        let banner = format!(
            "PALIMPSEST {} — {} commits · {} branches · {} tags — live region below; c/E/R commit finalized artifacts above it",
            env!("CARGO_PKG_VERSION"),
            app.hist.len(),
            app.branches.iter().filter(|b| !b.is_remote).count(),
            app.tags.len()
        );
        let _ = ctx.insert_text_before_live(&banner);
    }

    loop {
        // frame if dirty
        if app.dirty {
            flush_scrollback(app, &mut ctx);
            let pal = app.palette;
            let tree = views::build_ui(app, &pal);
            ctx.set_root(tree);
            app.dirty = false;
            if ctx.render().is_err() {
                break;
            }
        }

        // bounded wait; animation shortens it
        let wait = if app.anim.is_some() {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(120)
        };
        match ctx.poll_event(wait) {
            Ok(Some(Event::Key(k))) => {
                if !app::keys::handle_key(app, k) {
                    break;
                }
                // flush any artifact the key action queued
                flush_scrollback(app, &mut ctx);
            }
            Ok(Some(Event::Resize(w, h))) => {
                app.width = w.max(20);
                app.height = h.max(6);
                app.mark();
            }
            Ok(Some(Event::Paste(p))) => {
                for c in p.chars() {
                    if !app::keys::handle_key(app, gibson::input::KeyEvent::char(c)) {
                        break;
                    }
                }
                flush_scrollback(app, &mut ctx);
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("palimpsest: input error: {}", e);
                break;
            }
        }
        if app.tick_animation() && app.dirty {
            // rendered at the top of the next loop
        }
    }

    let _ = ctx.restore();

    if args.profile {
        report_profile(app, &mut ctx, "interactive");
    }
    // exit: Ok(()) by construction; nothing to report
}

/// Push queued finalized artifacts into native scrollback above the live
/// region. The application stays interactive below — this is the core
/// scrollback-integration contract of the workstation.
fn flush_scrollback(app: &mut App, ctx: &mut Context) {
    if app.scrollback_log.is_empty() {
        return;
    }
    while let Some(text) = app.scrollback_log.first().cloned() {
        // A rule before each artifact, editorial style.
        let _ = ctx.insert_text_before_live("───── palimpsest artifact ─────");
        // RichText keeps the artifact's line structure explicit.
        let mut rich = RichText::new();
        for l in text.lines() {
            let mut line = RLine::new();
            line.push(gibson::cell::Span::styled(
                l,
                gibson::cell::Style::new().dim(),
            ));
            rich.push_line(line);
        }
        let _ = ctx.insert_rich_text_before_live(&rich);
        app.scrollback_log.remove(0);
    }
}

// ---------------------------------------------------------------------------
// Headless dump (test / capture mode)
// ---------------------------------------------------------------------------

fn run_dump(app: &mut App, args: &Args) {
    let mut ctx = Context::headless(RenderMode::Inline, args.width, args.height);
    let depth = if args.mono || args.ansi16 {
        ColorDepth::Ansi16
    } else {
        ColorDepth::Ansi256
    };
    ctx.set_color_depth(depth);

    app.width = args.width;
    app.height = args.height;
    app.mark();

    // Render the requested view.
    if app.dirty {
        let pal = app.palette;
        let tree = views::build_ui(app, &pal);
        ctx.set_root(tree);
        app.dirty = false;
        let _ = ctx.render();
    }
    let frame = ctx.take_output();

    // Emit one of each scrollback artifact after the frame, so captures prove
    // the full pipeline (frame + artifact + re-rendered live region).
    app::keys::emit_dossier(app);
    flush_scrollback(app, &mut ctx);
    if app.dirty {
        let pal = app.palette;
        let tree = views::build_ui(app, &pal);
        ctx.set_root(tree);
        app.dirty = false;
        let _ = ctx.render();
    }
    let after = ctx.take_output();

    let mut out = std::io::stdout();
    let _ = out.write_all(frame.as_bytes());
    let _ = out.write_all(b"\n");
    let _ = out.write_all(after.as_bytes());
    let _ = out.flush();

    if args.profile {
        report_profile(app, &mut ctx, "dump");
    }
}

fn report_profile(app: &App, ctx: &mut Context, mode: &str) {
    let stats = ctx.stats();
    eprintln!("──── palimpsest profile ({}) ────", mode);
    eprintln!(
        "index: {} rows loaded, {} seen (truncated: {}), lanes {}",
        app.hist.len(),
        app.scanned,
        app.hist.truncated,
        app.hist.lanes
    );
    eprintln!(
        "load: {}ms · tier-A resolved in this session: {} · windows drawn: {}",
        app.profile.load_ms, app.profile.tier_a_resolved, app.profile.windows_drawn
    );
    eprintln!(
        "render: {} frames, {} full repaints, {} dirty cells, last frame {}µs",
        stats.frames, stats.full_repaints, stats.dirty_cells, stats.last_render_duration_micros
    );
    eprintln!(
        "bytes: frames {} · insertions {} ({}) · commits {}",
        stats.frame_bytes, stats.insertion_bytes, stats.history_insertions, stats.commit_bytes
    );
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_from_str_accepts_names_and_numbers() {
        assert_eq!(view_from_str("atlas"), Some(View::Atlas));
        assert_eq!(view_from_str("1"), Some(View::Atlas));
        assert_eq!(view_from_str("strata"), Some(View::Strata));
        assert_eq!(view_from_str("file"), Some(View::Strata));
        assert_eq!(view_from_str("lens"), Some(View::Lens));
        assert_eq!(view_from_str("diff"), Some(View::Lens));
        assert_eq!(view_from_str("provenance"), Some(View::Provenance));
        assert_eq!(view_from_str("blame"), Some(View::Provenance));
        assert_eq!(view_from_str("prov"), Some(View::Provenance));
        assert_eq!(view_from_str("health"), Some(View::Health));
        assert_eq!(view_from_str("metrics"), Some(View::Health));
        assert_eq!(
            view_from_str("ATLAS"),
            Some(View::Atlas),
            "case-insensitive"
        );
        assert_eq!(view_from_str("nope"), None);
        assert_eq!(view_from_str(""), None);
    }

    #[test]
    fn discover_repo_walks_upward_and_fails_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        // not a repo anywhere in this temp chain
        let err = discover_repo(tmp.path()).unwrap_err();
        assert!(
            err.contains("no repository found"),
            "readable error, got: {err}"
        );

        // nested repo discovered from a subdirectory
        let repo_dir = tmp.path().join("proj");
        std::fs::create_dir_all(&repo_dir).unwrap();
        git2::Repository::init(&repo_dir).unwrap();
        let deep = repo_dir.join("src").join("core");
        std::fs::create_dir_all(&deep).unwrap();
        let found = discover_repo(&deep).unwrap();
        assert_eq!(found, repo_dir, "discovery walks up to the worktree root");
    }

    #[test]
    fn discover_repo_accepts_worktree_git_files() {
        // a .git *file* (linked worktree) must also count as a repo
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("wt");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let real_git = tmp.path().join("real").join("objects");
        std::fs::create_dir_all(&real_git).unwrap();
        std::fs::write(
            repo_dir.join(".git"),
            format!("gitdir: {}\n", real_git.parent().unwrap().display()),
        )
        .unwrap();
        assert!(
            discover_repo(&repo_dir).is_ok(),
            "worktree .git file counts"
        );
    }
}
