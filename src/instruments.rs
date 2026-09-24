//! Provisional consumer-owned instrument IR. No generated code or terminal authority.
use crate::{
    semantic_fixture::{self as sem, Action, Snapshot, TimedAction},
    ui::{self, App},
};
use gibson::{
    input::{KeyCode, KeyEvent},
    BorderType, Color, ColorDepth, FocusId, FocusRing, Node, Rect, Style, Surface,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_JSON_BYTES: usize = 32_768;
pub const MAX_NODES: usize = 64;
pub const MAX_DEPTH: usize = 8;
pub const MAX_RASTER: u16 = 160;
pub const MAX_EFFECTS: usize = 0; // This first IR deliberately admits no effect programs.
pub const MAX_RATE: u8 = 30;
pub const MAX_HISTORY: usize = 64;
pub const MAX_MOUNTS: usize = 4;
pub const MAX_OPERATIONS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub nodes: usize,
    pub depth: usize,
    pub raster_width: u16,
    pub raster_height: u16,
    pub effects: usize,
    pub animation_hz: u8,
    pub retained_history: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            nodes: MAX_NODES,
            depth: MAX_DEPTH,
            raster_width: 80,
            raster_height: 24,
            effects: 0,
            animation_hz: 0,
            retained_history: 16,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Element {
    Text {
        id: String,
        text: String,
    },
    Column {
        id: String,
        children: Vec<Element>,
    },
    Row {
        id: String,
        children: Vec<Element>,
    },
    Panel {
        id: String,
        title: String,
        children: Vec<Element>,
    },
    Choice {
        id: String,
        label: String,
    },
    Lines {
        id: String,
        width: u16,
        height: u16,
        segments: Vec<[u16; 4]>,
        fallback: String,
    },
}
impl Element {
    pub fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::Column { id, .. }
            | Self::Row { id, .. }
            | Self::Panel { id, .. }
            | Self::Choice { id, .. }
            | Self::Lines { id, .. } => id,
        }
    }
    fn children(&self) -> &[Self] {
        match self {
            Self::Column { children, .. }
            | Self::Row { children, .. }
            | Self::Panel { children, .. } => children,
            _ => &[],
        }
    }
    fn walk<'a>(&'a self, out: &mut Vec<&'a Self>) {
        out.push(self);
        for c in self.children() {
            c.walk(out)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub node: String,
    pub subscription: String,
    pub action: InstrumentAction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstrumentAction {
    Select { target: String },
    ResolvePermission { approved: bool },
    ChooseScope { expedition: bool },
}
impl InstrumentAction {
    fn semantic(&self) -> Action {
        match self {
            Self::Select { target } => Action::Select {
                target: target.clone(),
            },
            Self::ResolvePermission { approved } => Action::ResolvePermission {
                approved: *approved,
            },
            Self::ChooseScope { expedition } => Action::ChooseScope {
                expedition: *expedition,
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentSpec {
    pub id: String,
    pub title: String,
    pub root: Element,
    pub bindings: Vec<Binding>,
    pub budget: Budget,
    pub mono_fallback: String,
    pub replay: bool,
    pub seed: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentPatch {
    pub root: Element,
    pub bindings: Vec<Binding>,
    pub title: String,
}
fn safe(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().any(char::is_control)
}
fn identity(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// Byte bound precedes JSON decoding, then serde's recursion/type checks and
/// our bounded tree/resource/action validation precede any mount or allocation.
pub fn decode(input: &[u8], state: &Snapshot) -> Result<InstrumentSpec, String> {
    if input.len() > MAX_JSON_BYTES {
        return Err("JSON byte budget exceeded".into());
    }
    let spec: InstrumentSpec =
        serde_json::from_slice(input).map_err(|e| format!("invalid schema: {e}"))?;
    validate(&spec, state)?;
    Ok(spec)
}
pub fn validate(spec: &InstrumentSpec, state: &Snapshot) -> Result<(), String> {
    let b = &spec.budget;
    if !identity(&spec.id) || !safe(&spec.title, 160) || spec.title.is_empty() {
        return Err("invalid instrument identity/title".into());
    }
    if b.nodes == 0 || b.nodes > MAX_NODES || b.depth == 0 || b.depth > MAX_DEPTH {
        return Err("node/depth budget exceeds authority".into());
    }
    if b.raster_width == 0
        || b.raster_height == 0
        || b.raster_width > MAX_RASTER
        || b.raster_height > MAX_RASTER
    {
        return Err("raster budget exceeds authority".into());
    }
    if b.effects > MAX_EFFECTS {
        return Err("effect programs unsupported by this IR".into());
    }
    if b.animation_hz > MAX_RATE {
        return Err("animation rate exceeds authority".into());
    }
    // Only static declarative content is supported; reject an admitted rate which
    // would otherwise promise animation that the compiler does not perform.
    if b.animation_hz != 0 {
        return Err("nonzero animation is unsupported".into());
    }
    if b.retained_history == 0 || b.retained_history > MAX_HISTORY {
        return Err("retained history exceeds authority".into());
    }
    if spec.mono_fallback.is_empty() || !safe(&spec.mono_fallback, 240) {
        return Err("missing/invalid Mono fallback".into());
    }
    if spec.replay && spec.seed.is_none() {
        return Err("replay needs explicit deterministic seed".into());
    }
    fn visit<'a>(
        n: &'a Element,
        d: usize,
        b: &Budget,
        ids: &mut BTreeSet<&'a str>,
        choices: &mut BTreeSet<&'a str>,
        raster_area: &mut usize,
    ) -> Result<(), String> {
        if d > b.depth {
            return Err("tree depth exceeded".into());
        }
        if !identity(n.id()) || !ids.insert(n.id()) {
            return Err("invalid/duplicate node ID".into());
        }
        if ids.len() > b.nodes {
            return Err("tree node count exceeded".into());
        }
        match n {
            Element::Text { text, .. } if !safe(text, 2048) => {
                return Err("unsafe/oversized text".into())
            }
            Element::Panel { title, .. } if !safe(title, 160) => {
                return Err("unsafe panel title".into())
            }
            Element::Choice { label, .. } => {
                if !safe(label, 160) {
                    return Err("unsafe choice label".into());
                }
                choices.insert(n.id());
            }
            Element::Lines {
                width,
                height,
                segments,
                fallback,
                ..
            } => {
                *raster_area = raster_area.saturating_add(*width as usize * *height as usize);
                if *raster_area > MAX_RASTER as usize * MAX_RASTER as usize {
                    return Err("aggregate raster area exceeded".into());
                }
                if *width == 0
                    || *height == 0
                    || *width > b.raster_width
                    || *height > b.raster_height
                    || segments.len() > 128
                {
                    return Err("line raster budget exceeded".into());
                }
                if fallback.is_empty() || !safe(fallback, 240) {
                    return Err("line raster needs textual fallback".into());
                }
                if segments
                    .iter()
                    .any(|s| s[0] >= *width || s[2] >= *width || s[1] >= *height || s[3] >= *height)
                {
                    return Err("line coordinate outside declared raster".into());
                }
            }
            _ => {}
        }
        for c in n.children() {
            visit(c, d + 1, b, ids, choices, raster_area)?
        }
        Ok(())
    }
    let mut ids = BTreeSet::new();
    let mut choices = BTreeSet::new();
    visit(&spec.root, 1, b, &mut ids, &mut choices, &mut 0)?;
    let mut bound = BTreeSet::new();
    for binding in &spec.bindings {
        if binding.subscription != "activate"
            || !choices.contains(binding.node.as_str())
            || !bound.insert(binding.node.as_str())
        {
            return Err("invalid/duplicate interaction subscription".into());
        }
        sem::validate_action(state, &binding.action.semantic())
            .map_err(|e| format!("invalid action target: {e}"))?;
    }
    if choices != bound {
        return Err("every choice must have exactly one binding".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lifecycle {
    Mount(InstrumentSpec),
    Update { id: String, patch: InstrumentPatch },
    Unmount(String),
    Focus(String),
    Action(TimedAction),
    Output(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPort {
    at_ms: u64,
    mounted: BTreeMap<String, InstrumentSpec>,
    actions: Vec<TimedAction>,
    focus: FocusRing,
    ids: BTreeMap<String, FocusId>,
    next_id: u64,
    prior: Vec<String>,
    history: Vec<Lifecycle>,
    output: Vec<String>,
}
impl Default for SessionPort {
    fn default() -> Self {
        Self {
            at_ms: 0,
            mounted: BTreeMap::new(),
            actions: vec![],
            focus: FocusRing::default(),
            ids: BTreeMap::new(),
            next_id: 1,
            prior: vec![],
            history: vec![],
            output: vec![],
        }
    }
}
impl SessionPort {
    pub fn read_snapshot(&self) -> Snapshot {
        sem::snapshot(self.at_ms, &self.actions)
    }
    pub fn at_ms(&self) -> u64 {
        self.at_ms
    }
    pub fn advance(&mut self, at_ms: u64) -> Result<(), String> {
        if at_ms < self.at_ms {
            return Err("time cannot regress in a live port".into());
        }
        self.at_ms = at_ms.min(sem::DURATION_MS);
        Ok(())
    }
    pub fn mounted(&self) -> &BTreeMap<String, InstrumentSpec> {
        &self.mounted
    }
    pub fn actions(&self) -> &[TimedAction] {
        &self.actions
    }
    pub fn history(&self) -> &[Lifecycle] {
        &self.history
    }
    pub fn output(&self) -> &[String] {
        &self.output
    }
    pub fn focused(&self) -> Option<&str> {
        let id = self.focus.current()?;
        self.ids
            .iter()
            .find_map(|(key, value)| (*value == id).then_some(key.as_str()))
    }
    fn record(&mut self, op: Lifecycle) {
        if self.history.len() == MAX_OPERATIONS {
            self.history.remove(0);
        }
        self.history.push(op)
    }
    fn refresh(&mut self) -> Result<(), String> {
        let old = self.focused().map(str::to_string);
        let mut current = vec![];
        for (id, spec) in &self.mounted {
            let mut nodes = vec![];
            spec.root.walk(&mut nodes);
            for n in nodes {
                if matches!(n, Element::Choice { .. }) {
                    current.push(format!("{id}/{}", n.id()))
                }
            }
        }
        if self.ids.len()
            + current
                .iter()
                .filter(|id| !self.ids.contains_key(*id))
                .count()
            > MAX_NODES * MAX_MOUNTS
        {
            return Err("lifetime focus identity budget exhausted".into());
        }
        let mut ring = vec![];
        for id in &current {
            let next = self.next_id;
            let v = self.ids.entry(id.clone()).or_insert_with(|| {
                self.next_id += 1;
                FocusId(next)
            });
            ring.push(*v)
        }
        self.focus = FocusRing::new(ring);
        if let Some(id) = old.and_then(|id| self.ids.get(&id)) {
            if self.focus.set(*id) {
                return Ok(());
            }
        }
        while let Some(key) = self.prior.pop() {
            if let Some(id) = self.ids.get(&key) {
                if self.focus.set(*id) {
                    break;
                }
            }
        }
        Ok(())
    }
    fn transaction(
        &mut self,
        edit: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut next = self.clone();
        edit(&mut next)?;
        *self = next;
        Ok(())
    }
    pub fn mount(&mut self, spec: InstrumentSpec) -> Result<(), String> {
        validate(&spec, &self.read_snapshot())?;
        if self.mounted.len() >= MAX_MOUNTS || self.mounted.contains_key(&spec.id) {
            return Err("mount capacity/duplicate instrument".into());
        }
        self.transaction(|next| {
            next.mounted.insert(spec.id.clone(), spec.clone());
            next.refresh()?;
            next.record(Lifecycle::Mount(spec));
            Ok(())
        })
    }
    pub fn update(&mut self, id: &str, patch: InstrumentPatch) -> Result<(), String> {
        let mut spec = self.mounted.get(id).cloned().ok_or("unknown instrument")?;
        spec.root = patch.root.clone();
        spec.bindings = patch.bindings.clone();
        spec.title = patch.title.clone();
        validate(&spec, &self.read_snapshot())?;
        self.transaction(|next| {
            next.mounted.insert(id.into(), spec);
            next.refresh()?;
            next.record(Lifecycle::Update {
                id: id.into(),
                patch,
            });
            Ok(())
        })
    }
    pub fn unmount(&mut self, id: &str) -> Result<(), String> {
        if !self.mounted.contains_key(id) {
            return Err("unknown instrument".into());
        }
        self.transaction(|next| {
            next.mounted.remove(id);
            next.refresh()?;
            next.record(Lifecycle::Unmount(id.into()));
            Ok(())
        })
    }
    pub fn request_focus(&mut self, key: &str) -> Result<(), String> {
        let id = *self.ids.get(key).ok_or("unknown focus identity")?;
        if !self.focus.contains(id) {
            return Err("focus target is not mounted".into());
        }
        if self.focused() != Some(key) {
            if let Some(old) = self.focused().map(str::to_string) {
                if self.prior.len() == MAX_HISTORY {
                    self.prior.remove(0);
                }
                self.prior.push(old)
            }
            self.focus.set(id);
            self.record(Lifecycle::Focus(key.into()));
        }
        Ok(())
    }
    pub fn cycle(&mut self, back: bool) {
        let mut ring = self.focus.clone();
        let id = if back {
            ring.focus_prev()
        } else {
            ring.focus_next()
        };
        if let Some(key) = id.and_then(|id| {
            self.ids
                .iter()
                .find_map(|(k, v)| (*v == id).then(|| k.clone()))
        }) {
            let _ = self.request_focus(&key);
        }
    }
    /// Only an explicitly activated, currently mounted binding can emit action.
    pub fn activate(&mut self, key: &str) -> Result<(), String> {
        let (instrument, node) = key.split_once('/').ok_or("invalid interaction identity")?;
        let action = self
            .mounted
            .get(instrument)
            .and_then(|s| {
                s.bindings
                    .iter()
                    .find(|b| b.node == node && b.subscription == "activate")
            })
            .ok_or("not an allowed interaction subscription")?
            .action
            .semantic();
        sem::validate_action(&self.read_snapshot(), &action)?;
        if self.actions.len() >= sem::MAX_ACTIONS {
            return Err("semantic action budget exhausted".into());
        }
        let timed = TimedAction {
            at_ms: self.at_ms,
            action,
        };
        self.actions.push(timed.clone());
        self.record(Lifecycle::Action(timed));
        Ok(())
    }
    /// Structured plain receipt only. This is retained lab output, not terminal I/O.
    /// The shared receipt rail is truncated to the calling instrument's budget.
    pub fn commit_output(&mut self, instrument: &str, text: &str) -> Result<(), String> {
        let limit = self
            .mounted
            .get(instrument)
            .ok_or("unknown output authority")?
            .budget
            .retained_history;
        if !safe(text, 512) {
            return Err("unsafe/oversized output".into());
        }
        while self.output.len() >= limit {
            self.output.remove(0);
        }
        self.output.push(text.into());
        self.record(Lifecycle::Output(text.into()));
        Ok(())
    }
    pub fn sync_plan(&mut self) -> Result<(), String> {
        let specs = plan(&self.read_snapshot(), self.at_ms);
        let ids: BTreeSet<_> = specs.iter().map(|s| s.id.clone()).collect();
        let obsolete: Vec<_> = self
            .mounted
            .keys()
            .filter(|k| !ids.contains(*k))
            .cloned()
            .collect();
        // Batch changes are atomic, including focus-ID allocation exhaustion.
        self.transaction(|next| {
            for id in obsolete {
                next.unmount(&id)?
            }
            for spec in specs {
                if let Some(old) = next.mounted.get(&spec.id) {
                    if *old != spec {
                        next.update(
                            &spec.id,
                            InstrumentPatch {
                                root: spec.root,
                                bindings: spec.bindings,
                                title: spec.title,
                            },
                        )?
                    }
                } else {
                    next.mount(spec)?
                }
            }
            Ok(())
        })
    }
}

fn text(id: &str, s: impl Into<String>) -> Element {
    Element::Text {
        id: id.into(),
        text: s.into(),
    }
}
fn lines(id: &str, segments: Vec<[u16; 4]>, fallback: &str) -> Element {
    Element::Lines {
        id: id.into(),
        width: 32,
        height: 8,
        segments,
        fallback: fallback.into(),
    }
}
fn base(id: &str, title: &str, children: Vec<Element>, bindings: Vec<Binding>) -> InstrumentSpec {
    InstrumentSpec {
        id: id.into(),
        title: title.into(),
        root: Element::Column {
            id: "root".into(),
            children,
        },
        bindings,
        budget: Budget::default(),
        mono_fallback: "Labels, line geometry and reverse focus retain meaning without color"
            .into(),
        replay: true,
        seed: Some(71),
    }
}
/// A deterministic representation planner over immutable semantic state.
/// These four probe representations are design examples, never later holdouts.
pub fn plan(state: &Snapshot, at_ms: u64) -> Vec<InstrumentSpec> {
    let mut choices = vec![];
    let mut bindings = vec![];
    for agent in &state.agents {
        choices.push(Element::Choice {
            id: agent.id.clone(),
            label: format!(
                "{} {:3}% {}",
                agent.title,
                agent.progress,
                if agent.failed {
                    "FAIL"
                } else if agent.finished {
                    "DONE"
                } else {
                    "LIVE"
                }
            ),
        });
        bindings.push(Binding {
            node: agent.id.clone(),
            subscription: "activate".into(),
            action: InstrumentAction::Select {
                target: agent.id.clone(),
            },
        });
    }
    let (title, diagram, detail) = if at_ms < 4600 {
        (
            "DEPENDENCY EXPLORER",
            lines(
                "graph",
                vec![[16, 0, 3, 6], [16, 0, 16, 6], [16, 0, 29, 6]],
                "ARCHITECT → SCOUT | BUILDER | VERIFY",
            ),
            "One goal; parallel witnesses share one fixture revision".into(),
        )
    } else if at_ms < 9000 {
        let marks = (state.event_order.len().min(15)) as u16;
        let mut segments = vec![[0, 4, 31, 4]];
        for x in 0..marks {
            segments.push([x * 2, 2, x * 2, 6]);
        }
        (
            "TIMELINE",
            lines(
                "graph",
                segments,
                "Event order → failure → explicit local recovery",
            ),
            format!(
                "{} ordered receipts; failure is retained, not rewritten",
                state.event_order.len()
            ),
        )
    } else if at_ms < 9990 || state.finished {
        (
            "SOURCE / PROVENANCE MAP",
            lines(
                "graph",
                vec![[1, 1, 14, 4], [1, 7, 14, 4], [14, 4, 30, 4]],
                "supplied map + access receipts → replay → review",
            ),
            format!(
                "{} artifacts; source events = {}",
                state.artifacts.len(),
                state.event_order.len()
            ),
        )
    } else {
        (
            "DECISION INSPECTOR",
            lines(
                "graph",
                vec![[2, 4, 14, 4], [14, 4, 29, 1], [14, 4, 29, 7]],
                "permission → approve / decline; explicit action only",
            ),
            format!(
                "Permission: {}",
                if state.permission_pending {
                    "PENDING: Y/N"
                } else if state.permission == Some(false) {
                    "DECLINED"
                } else {
                    "APPROVED"
                }
            ),
        )
    };
    let mut workspace = vec![
        text("description", detail),
        diagram,
        Element::Column {
            id: "workers".into(),
            children: choices,
        },
    ];
    if state.permission_pending {
        for (id, label, approved) in [
            ("allow", "Y / approve fictional artifact", true),
            ("deny", "N / withhold fictional artifact", false),
        ] {
            workspace.push(Element::Choice {
                id: id.into(),
                label: label.into(),
            });
            bindings.push(Binding {
                node: id.into(),
                subscription: "activate".into(),
                action: InstrumentAction::ResolvePermission { approved },
            });
        }
    }
    let selected = state.selected.as_deref().unwrap_or("architect");
    let detail = state
        .agents
        .iter()
        .find(|a| a.id == selected)
        .map_or("No agent has started", |a| a.summary.as_str());
    let inspector = base(
        "inspector",
        "LIVE SELECTION",
        vec![
            text("selected", format!("Selected: {selected}")),
            text("summary", detail),
            text("receipt", &state.receipt),
        ],
        vec![],
    );
    let mut all = vec![base("workspace", title, workspace, bindings), inspector];
    // A transient read-only permission instrument exercises actual mount/unmount.
    if state.permission_pending {
        all.push(base("permission","AUTHORITY BOUNDARY",vec![text("notice","Generated instruments cannot run tools. Y/N emits a recorded local permission action; no filesystem access.")],vec![]));
    }
    all
}

fn compile(n: &Element, scope: &str, port: &SessionPort, depth: ColorDepth) -> Node {
    let focused = port.focused() == Some(format!("{scope}/{}", n.id()).as_str());
    match n {
        Element::Text { text, .. } => ui::text(text, Color::White),
        Element::Choice { label, .. } => Node::text(
            format!("{} {label}", if focused { ">" } else { " " }),
            if focused {
                Style::new().fg(Color::Cyan).bold().reverse()
            } else {
                Style::new().fg(Color::White)
            },
        ),
        Element::Column { children, .. } => children.iter().fold(Node::col(), |out, c| {
            out.child(compile(c, scope, port, depth))
        }),
        Element::Row { children, .. } => children.iter().fold(Node::row(), |out, c| {
            out.child(compile(c, scope, port, depth).flex_grow(1.))
        }),
        Element::Panel {
            title, children, ..
        } => children.iter().fold(
            Node::panel(title, BorderType::Rounded, Style::new().fg(Color::Cyan)),
            |out, c| out.child(compile(c, scope, port, depth)),
        ),
        Element::Lines {
            width,
            height,
            segments,
            fallback,
            ..
        } => {
            let mut canvas = gibson::canvas::BrailleCanvas::new(*width, *height / 2);
            for [x0, y0, x1, y1] in segments {
                canvas.line(
                    *x0 as i32 * 2,
                    *y0 as i32 * 2,
                    *x1 as i32 * 2,
                    *y1 as i32 * 2,
                )
            }
            Node::col()
                .child(Node::raster(canvas.to_surface(Style::new().fg(
                    if depth == ColorDepth::Mono {
                        Color::White
                    } else {
                        Color::Cyan
                    },
                ))))
                .child(ui::text(fallback, Color::Yellow))
        }
    }
}
impl SessionPort {
    pub fn frame(&self, w: u16, h: u16, depth: ColorDepth) -> Surface {
        let (w, h) = ui::dimensions(w, h);
        let mut out = Surface::new(w, h);
        if w < 32 || h < 18 {
            ui::put(
                &mut out,
                0,
                0,
                "INSTRUMENTS / enlarge to 32x18",
                Style::new().bold(),
            );
            return ui::finish(out, depth);
        }
        ui::put(
            &mut out,
            1,
            0,
            "INSTRUMENT LAB / generated, bounded, inspectable",
            Style::new().fg(Color::Cyan).bold(),
        );
        let primary = self
            .focused()
            .and_then(|key| key.split_once('/').map(|p| p.0))
            .filter(|id| self.mounted.contains_key(*id))
            .or_else(|| {
                self.mounted
                    .contains_key("workspace")
                    .then_some("workspace")
            })
            .or_else(|| self.mounted.keys().next().map(String::as_str));
        let side = if w >= 100 { w / 3 } else { 0 };
        let left = w - 2 - side;
        if let Some(spec) = primary.and_then(|id| self.mounted.get(id)) {
            let panel = Node::panel(
                &spec.title,
                BorderType::Double,
                Style::new().fg(Color::Cyan),
            )
            .child(compile(&spec.root, &spec.id, self, depth));
            ui::mount(
                &mut out,
                panel,
                Rect::new(1, 2, left, if side > 0 { h - 4 } else { h - 10 }),
            );
        }
        let auxiliary: Vec<_> = self
            .mounted
            .values()
            .filter(|spec| Some(spec.id.as_str()) != primary)
            .collect();
        let region = if side > 0 {
            Rect::new(1 + left, 2, side, h - 4)
        } else {
            Rect::new(1, h - 8, w - 2, 6)
        };
        let count = auxiliary.len().max(1) as u16;
        for (index, spec) in auxiliary.iter().enumerate() {
            let start = region.height * index as u16 / count;
            let end = region.height * (index as u16 + 1) / count;
            let panel = Node::panel(
                &spec.title,
                BorderType::Rounded,
                Style::new().fg(ui::accent(index + 1)),
            )
            .child(compile(&spec.root, &spec.id, self, depth));
            ui::mount(
                &mut out,
                panel,
                Rect::new(region.x, region.y + start, region.width, end - start),
            );
        }
        ui::put(
            &mut out,
            2,
            1,
            &format!(
                "{} mounted · active {}",
                self.mounted.len(),
                primary.unwrap_or("none")
            ),
            Style::new().dim(),
        );
        if let Some(receipt) = self.output.last() {
            ui::put(&mut out, 1, h - 2, receipt, Style::new().fg(Color::Yellow));
        }
        ui::put(
            &mut out,
            1,
            h - 1,
            "Tab/↑/↓ focus · Enter inspect · Y/N permission",
            Style::new().dim(),
        );
        ui::finish(out, depth)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    Next,
    Previous,
    Activate,
    Allow,
    Deny,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub at_ms: u64,
    pub intent: Intent,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instruments {
    pub inputs: Vec<Input>,
    pub rejection: Option<String>,
}
impl Instruments {
    /// Reconstruct semantic state AND mount/update/focus history from ordered
    /// inputs plus the same immutable source events. Paint never mutates history.
    pub fn replay(&self, at_ms: u64) -> Result<SessionPort, String> {
        if self.inputs.len() > MAX_HISTORY
            || self
                .inputs
                .iter()
                .any(|input| input.at_ms > sem::DURATION_MS)
            || self.inputs.windows(2).any(|s| s[0].at_ms > s[1].at_ms)
        {
            return Err("invalid input budget/order".into());
        }
        let mut port = SessionPort::default();
        let mut times: Vec<u64> = sem::fixture()
            .into_iter()
            .map(|s| s.at_ms)
            .filter(|t| *t <= at_ms)
            .collect();
        times.extend(
            self.inputs
                .iter()
                .filter(|i| i.at_ms <= at_ms)
                .map(|i| i.at_ms),
        );
        times.push(at_ms.min(sem::DURATION_MS));
        times.sort_unstable();
        times.dedup();
        for t in times {
            port.advance(t)?;
            port.sync_plan()?;
            for input in self.inputs.iter().filter(|i| i.at_ms == t) {
                let result = match input.intent {
                    Intent::Next => {
                        port.cycle(false);
                        Ok(())
                    }
                    Intent::Previous => {
                        port.cycle(true);
                        Ok(())
                    }
                    Intent::Activate => match port.focused().map(str::to_string) {
                        Some(key) => port.activate(&key),
                        None => Err("no active interaction target".into()),
                    },
                    Intent::Allow => port.activate("workspace/allow"),
                    Intent::Deny => port.activate("workspace/deny"),
                };
                if let Err(reason) = result {
                    port.commit_output("workspace", &format!("REJECTED: {reason}"))?;
                }
                port.sync_plan()?
            }
        }
        if let Some(reason) = &self.rejection {
            port.commit_output("workspace", reason)?;
        }
        Ok(port)
    }
}
impl App for Instruments {
    fn frame(&self, at_ms: u64, w: u16, h: u16, depth: ColorDepth) -> Surface {
        match self.replay(at_ms) {
            Ok(port) => port.frame(w, h, depth),
            Err(reason) => ui::finish(
                ui::realize(ui::text(format!("REJECTED: {reason}"), Color::Red), w, h),
                depth,
            ),
        }
    }
    fn key(&mut self, at_ms: u64, key: KeyEvent) {
        let intent = match key.code {
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => Intent::Next,
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => Intent::Previous,
            KeyCode::Enter => Intent::Activate,
            KeyCode::Char('y' | 'Y') => Intent::Allow,
            KeyCode::Char('n' | 'N') => Intent::Deny,
            _ => return,
        };
        if self.inputs.len() == MAX_HISTORY {
            self.rejection = Some("REJECTED: input history budget exhausted".into());
            return;
        }
        if self.inputs.last().is_some_and(|i| i.at_ms > at_ms) {
            self.rejection = Some("REJECTED: input time regressed".into());
            return;
        }
        self.inputs.push(Input {
            at_ms: at_ms.min(sem::DURATION_MS),
            intent,
        });
        self.rejection = None;
    }
}
