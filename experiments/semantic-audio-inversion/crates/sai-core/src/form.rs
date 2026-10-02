//! Form inference: candidate section boundaries from evidence and a **family topology** that is
//! never a human section label.
//!
//! HumanMusic's dial orders form relations `Free < Topology < Exact`. Topology means the ordered
//! sequence of section families without bar counts; Exact means the phrase spans too. Audio
//! rarely supports Exact, so the recovery ceiling is normally Topology — and this module says so.
//!
//! Repeated material alone must not manufacture semantic section labels: a section's identity here
//! is only a **family index** obtained from measured recurrence. No `Verse`/`Chorus` name is ever
//! asserted.

use serde::{Deserialize, Serialize};

use crate::error::{finite, SaiError, SaiResult};
use crate::evidence::{RecurrenceEvidence, SectionEvidence};
use crate::transport::BeatGrid;

/// Recurrence similarity above which two sections are placed in the same family.
pub const RECURRENCE_TOL: f64 = 0.70;

/// Exact relations on the form (mirrors HumanMusic's `FormRelation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormRelation {
    Free,
    /// The ordered section-family sequence without bar counts.
    Topology,
    /// Exact phrase spans and families.
    Exact,
}

impl FormRelation {
    pub fn label(self) -> &'static str {
        match self {
            FormRelation::Free => "free",
            FormRelation::Topology => "topology",
            FormRelation::Exact => "exact",
        }
    }
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "free" => Self::Free,
            "topology" => Self::Topology,
            "exact" => Self::Exact,
            _ => return None,
        })
    }
    pub fn rank(self) -> u8 {
        self as u8
    }
}

/// One inferred section, seconds authoritative, beats derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormSection {
    pub start_second: f64,
    pub end_second: f64,
    pub start_beat: f64,
    pub end_beat: f64,
    /// Cluster index (0-based, in first-appearance order); never a semantic name.
    pub family: usize,
}

/// The inferred form plus its provenance and ambiguity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormInference {
    pub provenance: crate::evidence::Provenance,
    pub sections: Vec<FormSection>,
    /// Compressed family sequence (consecutive duplicates collapsed).
    pub topology: Vec<usize>,
    /// How boundaries were obtained (e.g. `novelty+recurrence`).
    pub boundary_source: String,
    pub ambiguity: Vec<String>,
}

impl FormInference {
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}

/// Infer form from section and recurrence evidence.
pub fn infer_form(
    sections: &SectionEvidence,
    recurrence: Option<&RecurrenceEvidence>,
    grid: &BeatGrid,
) -> SaiResult<FormInference> {
    if sections.sections.is_empty() {
        return Err(SaiError::NoEvidence("section evidence"));
    }
    let mut secs: Vec<FormSection> = Vec::with_capacity(sections.sections.len());
    for sp in &sections.sections {
        let sb = grid.seconds_to_beat(sp.start_second)?.beat;
        let eb = grid.seconds_to_beat(sp.end_second)?.beat;
        secs.push(FormSection {
            start_second: sp.start_second,
            end_second: sp.end_second,
            start_beat: sb,
            end_beat: eb,
            family: 0,
        });
    }
    // Union-find over section indices, linked by measured recurrence.
    let n = secs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    let mut ambiguity = Vec::new();
    let mut linked = 0usize;
    if let Some(rec) = recurrence {
        for l in &rec.links {
            finite(l.similarity, "link.similarity")?;
            if l.similarity < RECURRENCE_TOL {
                continue;
            }
            let ia = section_at(&secs, l.at_second);
            let ib = section_at(&secs, l.to_second);
            if let (Some(a), Some(b)) = (ia, ib) {
                if a != b {
                    let ra = find(&mut parent, a);
                    let rb = find(&mut parent, b);
                    if ra != rb {
                        parent[ra] = rb;
                        linked += 1;
                    }
                }
            }
        }
    } else {
        ambiguity.push("no recurrence evidence: sections are kept as distinct families".into());
    }
    if linked == 0 {
        ambiguity.push("no section pair exceeded the recurrence floor: family topology is degenerate".into());
    }
    // Family ids in first-appearance order.
    let mut family_of_root: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        let id = match family_of_root.iter().find(|(root, _)| *root == r) {
            Some((_, id)) => *id,
            None => {
                let id = family_of_root.len();
                family_of_root.push((r, id));
                id
            }
        };
        secs[i].family = id;
    }
    let topology = compress(&secs.iter().map(|s| s.family).collect::<Vec<_>>());
    Ok(FormInference {
        provenance: sections.provenance.clone(),
        sections: secs,
        topology,
        boundary_source: "section-evidence+recurrence/v1".into(),
        ambiguity,
    })
}

fn section_at(secs: &[FormSection], second: f64) -> Option<usize> {
    secs.iter().position(|s| {
        second >= s.start_second - 1e-9 && second < s.end_second + 1e-9
    })
}

fn compress(seq: &[usize]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for &x in seq {
        if out.last() != Some(&x) {
            out.push(x);
        }
    }
    out
}

/// Compare two inferred forms under the declared relations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormComparison {
    pub relation: FormRelation,
    pub topology_equal: bool,
    pub spans_equal: bool,
    pub section_count_equal: bool,
}

pub fn compare_forms(a: &FormInference, b: &FormInference, span_tol_beats: f64) -> FormComparison {
    let topology_equal = a.topology == b.topology;
    let section_count_equal = a.sections.len() == b.sections.len();
    let spans_equal = section_count_equal
        && a.sections
            .iter()
            .zip(&b.sections)
            .all(|(x, y)| {
                (x.start_beat - y.start_beat).abs() <= span_tol_beats
                    && (x.end_beat - y.end_beat).abs() <= span_tol_beats
            });
    let relation = if a.is_empty() || b.is_empty() {
        FormRelation::Free
    } else if topology_equal && spans_equal {
        FormRelation::Exact
    } else if topology_equal {
        FormRelation::Topology
    } else {
        FormRelation::Free
    };
    FormComparison {
        relation,
        topology_equal,
        spans_equal,
        section_count_equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{Provenance, RecurrenceLink, SectionSpan};

    fn grid() -> BeatGrid {
        // Half-second beats for 16 beats.
        let beats: Vec<f64> = (0..17).map(|i| i as f64 * 0.5).collect();
        BeatGrid::from_beats(&beats, &[0.0, 4.0], 4).unwrap()
    }

    fn section_ev(sections: Vec<(f64, f64)>) -> SectionEvidence {
        SectionEvidence {
            provenance: Provenance::derived("test", "1", "sections/v1", &["pcm"]),
            boundaries: sections.iter().map(|(s, _)| *s).collect(),
            sections: sections
                .into_iter()
                .map(|(s, e)| SectionSpan {
                    start_second: s,
                    end_second: e,
                    label_candidate: None,
                })
                .collect(),
        }
    }

    #[test]
    fn recurrence_links_merge_families() {
        let ev = section_ev(vec![(0.0, 2.0), (2.0, 4.0), (4.0, 6.0)]);
        let rec = RecurrenceEvidence {
            provenance: Provenance::derived("test", "1", "rec/v1", &["pcm"]),
            links: vec![
                RecurrenceLink { at_second: 0.2, to_second: 4.2, similarity: 0.9 },
            ],
            summary: String::new(),
        };
        let f = infer_form(&ev, Some(&rec), &grid()).unwrap();
        assert_eq!(f.sections[0].family, f.sections[2].family);
        assert_ne!(f.sections[0].family, f.sections[1].family);
        assert_eq!(f.topology, vec![0, 1, 0]);
    }

    #[test]
    fn identical_forms_are_exact() {
        let ev = section_ev(vec![(0.0, 2.0), (2.0, 4.0)]);
        let f = infer_form(&ev, None, &grid()).unwrap();
        let c = compare_forms(&f, &f, 0.25);
        assert_eq!(c.relation, FormRelation::Exact);
    }

    #[test]
    fn different_families_lower_to_free() {
        let ev = section_ev(vec![(0.0, 2.0), (2.0, 4.0), (4.0, 6.0)]);
        let rec = RecurrenceEvidence {
            provenance: Provenance::derived("test", "1", "rec/v1", &["pcm"]),
            links: vec![
                RecurrenceLink { at_second: 0.2, to_second: 4.2, similarity: 0.9 },
            ],
            summary: String::new(),
        };
        let f = infer_form(&ev, Some(&rec), &grid()).unwrap();
        let other = infer_form(&ev, None, &grid()).unwrap();
        // distinct families 0,1,2 vs 0,1,0 -> topology differs -> Free
        assert_eq!(compare_forms(&f, &other, 0.25).relation, FormRelation::Free);
    }
}
