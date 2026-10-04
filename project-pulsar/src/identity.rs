//! Signal identity: every candidate keeps ONE identity (slot) across every
//! representation. Identity rides several redundant channels so it survives
//! Mono: a glyph shape (hollow while unconfirmed, solid once locked), a letter,
//! a state suffix, and a hue.

use gibson::{Color, Style};

use crate::sim::SLOTS;
use crate::track::State;

#[derive(Clone, Copy, Debug)]
pub struct Identity {
    pub letter: char,
    pub solid: &'static str,
    pub hollow: &'static str,
    pub rgb: (u8, u8, u8),
}

pub const IDENT: [Identity; SLOTS] = [
    Identity {
        letter: 'A',
        solid: "●",
        hollow: "○",
        rgb: (80, 222, 248),
    },
    Identity {
        letter: 'B',
        solid: "◆",
        hollow: "◇",
        rgb: (252, 196, 88),
    },
    Identity {
        letter: 'C',
        solid: "▲",
        hollow: "△",
        rgb: (236, 128, 236),
    },
    Identity {
        letter: 'D',
        solid: "■",
        hollow: "□",
        rgb: (238, 92, 80),
    },
    Identity {
        letter: 'E',
        solid: "▼",
        hollow: "▽",
        rgb: (132, 238, 120),
    },
];

pub fn glyph(slot: usize, state: State) -> &'static str {
    match state {
        State::Locked => IDENT[slot].solid,
        _ => IDENT[slot].hollow,
    }
}

pub fn suffix(state: State) -> &'static str {
    match state {
        State::Candidate => "?",
        State::Tracking => "~",
        State::Locked | State::Quiet => "",
        State::RejectedCw | State::RejectedTransient => "x",
    }
}

/// The compact identity tag, e.g. `●A`, `○B?`, `□Dx`.
pub fn tag(slot: usize, state: State) -> String {
    format!(
        "{}{}{}",
        glyph(slot, state),
        IDENT[slot].letter,
        suffix(state)
    )
}

/// Colour of an identity in a given state: unconfirmed and rejected identities
/// are desaturated so lock acquisition reads as the colour "turning on".
pub fn rgb(slot: usize, state: State) -> (u8, u8, u8) {
    let (r, g, b) = IDENT[slot].rgb;
    let k = match state {
        State::Locked => 1.0,
        State::Tracking => 0.78,
        State::Candidate | State::Quiet => 0.55,
        State::RejectedCw | State::RejectedTransient => 0.42,
    };
    let grey = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    let mix = |c: u8| {
        let desat = match state {
            State::RejectedCw | State::RejectedTransient => 0.6,
            State::Candidate | State::Quiet => 0.3,
            _ => 0.0,
        };
        let v = (c as f32 * (1.0 - desat) + grey * desat) * k;
        v.clamp(0.0, 255.0) as u8
    };
    (mix(r), mix(g), mix(b))
}

pub fn style(slot: usize, state: State) -> Style {
    let (r, g, b) = rgb(slot, state);
    let mut st = Style::new().fg(Color::rgb(r, g, b));
    if state == State::Locked {
        st.bold = true;
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_single_cell_and_distinct() {
        let mut seen = std::collections::HashSet::new();
        for id in IDENT {
            for g in [id.solid, id.hollow] {
                assert_eq!(unicode_width_of(g), 1, "{g}");
                assert!(seen.insert(g), "duplicate glyph {g}");
            }
        }
        let letters: std::collections::HashSet<char> = IDENT.iter().map(|i| i.letter).collect();
        assert_eq!(letters.len(), SLOTS);
    }

    fn unicode_width_of(s: &str) -> usize {
        gibson::Glyph::new(s).display_width as usize
    }

    #[test]
    fn lock_fills_the_glyph_and_changes_nothing_else_about_identity() {
        for (slot, id) in IDENT.iter().enumerate() {
            assert_eq!(glyph(slot, State::Candidate), id.hollow);
            assert_eq!(glyph(slot, State::Locked), id.solid);
            assert!(tag(slot, State::Locked).contains(id.letter));
            assert!(tag(slot, State::Candidate).contains(id.letter));
        }
    }
}
