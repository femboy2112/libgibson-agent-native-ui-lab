//! Sequential art over the common trace. Comic grammar stays in the consumer.
use crate::{
    semantic_fixture::{self, Action, TimedAction},
    ui::{self, App},
};
use gibson::input::{KeyCode, KeyEvent};
use gibson::{BorderType, Color, ColorDepth, FocusId, FocusRing, Node, Rect, Style, Surface};

#[derive(Clone, Debug)]
pub struct Manga {
    pub focus: FocusRing,
    pub actions: Vec<TimedAction>,
    pub notice: Option<String>,
}
impl Default for Manga {
    fn default() -> Self {
        Self {
            focus: FocusRing::new((1..=4).map(FocusId)),
            actions: Vec::new(),
            notice: None,
        }
    }
}
impl Manga {
    pub fn selected(&self) -> usize {
        self.focus
            .current()
            .map_or(0, |id| id.0.saturating_sub(1) as usize)
            .min(3)
    }
    fn act(&mut self, at_ms: u64, action: Action) {
        let permission = matches!(action, Action::ResolvePermission { .. });
        if self.actions.last().is_some_and(|a| a.at_ms > at_ms) {
            self.notice = Some("Action timestamps must be ordered".into());
            return;
        }
        let state = semantic_fixture::snapshot(at_ms, &self.actions);
        if let Err(error) = semantic_fixture::validate_action(&state, &action) {
            self.notice = Some(error);
            return;
        }
        if self.actions.len() >= if permission { 64 } else { 63 } {
            self.notice = Some("Action budget reached; one permission slot reserved".into());
            return;
        }
        self.actions.push(TimedAction { at_ms, action });
        self.notice = None;
    }
}
impl App for Manga {
    fn key(&mut self, at_ms: u64, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::Right => {
                self.focus.focus_next();
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.focus.focus_prev();
            }
            KeyCode::Enter => self.act(
                at_ms,
                Action::Select {
                    target: semantic_fixture::AGENT_IDS[self.selected()].into(),
                },
            ),
            KeyCode::Char('y' | 'Y') => {
                self.act(at_ms, Action::ResolvePermission { approved: true })
            }
            KeyCode::Char('n' | 'N') => {
                self.act(at_ms, Action::ResolvePermission { approved: false })
            }
            _ => {}
        }
    }
    fn frame(&self, at_ms: u64, w: u16, h: u16, depth: ColorDepth) -> Surface {
        let (w, h) = ui::dimensions(w, h);
        let mut out = Surface::new(w, h);
        if w < 24 || h < 18 {
            ui::put(
                &mut out,
                0,
                0,
                "MANGA · enlarge to 24x18",
                Style::new().bold(),
            );
            return ui::finish(out, depth);
        }
        let state = semantic_fixture::snapshot(at_ms, &self.actions);
        let selected = self.selected();
        ui::put(
            &mut out,
            1,
            0,
            "FIELD NOTES / a session in panels",
            Style::new().bold().fg(Color::Cyan),
        );
        let upper = Node::panel(
            "01 / THE ASK",
            BorderType::Rounded,
            Style::new().fg(Color::Cyan),
        )
        .child(ui::text(&state.goal, Color::White))
        .child(ui::text(
            if at_ms < 2000 {
                "A single request. Four different ways of seeing it."
            } else {
                "ONE GOAL → concurrent witnesses → one accountable result"
            },
            Color::BrightBlack,
        ));
        ui::mount(&mut out, upper, Rect::new(1, 2, w - 2, 5));
        let telemetry = if w >= 100 { 22 } else { 0 };
        let area_w = w - 2 - telemetry;
        let cols = if w >= 100 { 4 } else { 2 };
        let rows = 4 / cols;
        let middle_h = h - 13;
        let pw = (area_w - (cols as u16 - 1) * 2) / cols as u16;
        let ph = (middle_h - (rows as u16 - 1)) / rows as u16;
        let mut rects = Vec::new();
        for i in 0..4 {
            let x = 1 + (i % cols) as u16 * (pw + 2);
            let y = 8 + (i / cols) as u16 * (ph + 1);
            let rect = Rect::new(x, y, pw, ph);
            rects.push(rect);
            let agent = state
                .agents
                .iter()
                .find(|a| a.id == semantic_fixture::AGENT_IDS[i]);
            let color = ui::accent(i);
            let style = if i == selected {
                Style::new().fg(color).bold().reverse()
            } else {
                Style::new().fg(color)
            };
            let title = format!(
                "{} {} {}",
                if i == selected { ">" } else { " " },
                i + 1,
                semantic_fixture::AGENT_IDS[i].to_uppercase()
            );
            let mut card = Node::panel(
                title,
                if i == selected {
                    BorderType::Double
                } else {
                    BorderType::Rounded
                },
                style,
            );
            let p = agent.map_or(0, |a| a.progress);
            let bars = (p as usize * pw.saturating_sub(10) as usize / 100).min(60);
            card = card.child(ui::text(format!("{} {p}%", "━".repeat(bars)), color));
            card = card.child(ui::text(
                agent.map_or("Waiting for dispatch", |a| a.summary.as_str()),
                Color::White,
            ));
            if ph > 6 {
                card = card.child(ui::text(
                    if agent.is_some_and(|a| a.failed) {
                        "! a witness broke / recovery follows"
                    } else if agent.is_some_and(|a| a.finished) {
                        "✓ witness sealed"
                    } else {
                        "↔ shares the same time gutter"
                    },
                    color,
                ));
            }
            let fraction =
                ((at_ms.saturating_sub(1400 + i as u64 * 150)) as f32 / 650.).clamp(0., 1.);
            card = card.post_process([gibson::surface_fx::SurfaceFx::Dissolve {
                fraction,
                seed: i as u64 + 71,
            }]);
            ui::mount(&mut out, card, rect);
        }
        // Unplanned device: a causal repair seam. It stitches the failed worker
        // to the recovering worker, crossing the gutter instead of adding a log.
        if at_ms >= 6500 {
            let a = rects[1];
            let b = rects[2];
            let y = (a.y + a.height.saturating_sub(1)).min(h - 5);
            let x0 = a.x.min(b.x);
            let x1 = (a.x + a.width).max(b.x + b.width).min(w - 1);
            let repaired = at_ms >= 8500;
            for x in x0..x1 {
                ui::put(
                    &mut out,
                    x,
                    y,
                    if repaired {
                        if (x as u64 + at_ms / 150).is_multiple_of(3) {
                            "╳"
                        } else {
                            "─"
                        }
                    } else {
                        "╱"
                    },
                    Style::new()
                        .fg(if repaired { Color::Green } else { Color::Red })
                        .bold(),
                );
            }
            ui::put(
                &mut out,
                x0,
                y,
                if repaired {
                    "↳ REPAIR SEAM"
                } else {
                    "! TOOL FAILURE / gutter torn"
                },
                Style::new()
                    .fg(if repaired { Color::Green } else { Color::Red })
                    .reverse(),
            );
        }
        if telemetry > 0 {
            let panel = Node::panel("MARGINALIA", BorderType::Rounded, Style::new().dim())
                .child(ui::text(
                    format!(
                        "{} events\n{} workers\n{} artifacts",
                        state.event_order.len(),
                        state.agents.len(),
                        state.artifacts.len()
                    ),
                    Color::White,
                ))
                .child(ui::text(
                    "→ reading order\n↔ concurrent\n╳ repair witness",
                    Color::BrightBlack,
                ));
            ui::mount(&mut out, panel, Rect::new(w - 21, 8, 20, middle_h));
        }
        if state.finished {
            // The final page abandons concurrent columns for one synthesis
            // splash. The same witnesses now converge on an artifact decision.
            let mut ink = gibson::canvas::BrailleCanvas::new(w - 4, middle_h.saturating_sub(4));
            let cx = ink.pixel_width() as i32 / 2;
            let cy = ink.pixel_height() as i32 / 2;
            for i in 0..16 {
                let theta = i as f32 * std::f32::consts::TAU / 16.;
                ink.line(
                    cx + (theta.cos() * 12.) as i32,
                    cy + (theta.sin() * 7.) as i32,
                    cx + (theta.cos() * w as f32) as i32,
                    cy + (theta.sin() * middle_h as f32 * 3.) as i32,
                );
            }
            let splash = Node::panel(
                "05 / ALL FOUR WITNESSES CONVERGE",
                BorderType::Double,
                Style::new().fg(Color::Cyan).bold(),
            )
            .child(ui::text(
                if state.artifacts.is_empty() {
                    "WITHHELD / your permission choice is binding"
                } else {
                    "ONE REVIEW. EVERY CLAIM ACCOUNTABLE."
                },
                Color::White,
            ))
            .child(ui::text(
                "SCOUT indexed → BUILDER repaired → VERIFY replayed → ARCHITECT synthesized",
                Color::Yellow,
            ))
            .child(Node::raster(
                ink.to_surface(Style::new().fg(Color::BrightBlack)),
            ));
            ui::mount(&mut out, splash, Rect::new(1, 8, w - 2, middle_h));
        }
        let (title, receipt) = if let Some(notice) = &self.notice {
            ("INPUT REJECTED", notice.clone())
        } else if state.permission_pending {
            (
                "A REAL CHOICE",
                "Y allow local artifact / N deny — this emits a recorded action".to_string(),
            )
        } else if state.finished {
            ("SYNTHESIS / SPLASH", state.artifacts.join(" + "))
        } else {
            ("RECEIPT", state.receipt.clone())
        };
        let panel = Node::panel(title, BorderType::Double, Style::new().fg(Color::Yellow))
            .child(ui::text(receipt, Color::White));
        ui::mount(&mut out, panel, Rect::new(1, h - 5, w - 2, 4));
        ui::put(
            &mut out,
            1,
            h - 1,
            "Tab/←/→ panel · Enter inspect · Y/N permission · Space pause",
            Style::new().dim(),
        );
        ui::finish(out, depth)
    }
}
