//! The deterministic simulated distributed system.
//!
//! `Engine::step` advances one logical tick. It is a closed, self-contained stress
//! domain: no sockets, no clocks, no OS state. The same `(fixture, seed, journal)`
//! always produces the same sequence of [`Metrics`] and the same semantic digest.
//!
//! **This is a fictional domain.** It is not calibrated against, and makes no claim
//! about, any production distributed system.

use crate::action::{Action, ActionKind};
use crate::rng::Rng;
use crate::sim::fixture::{Fixture, ServiceId};

/// Circuit-breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breaker {
    Closed,
    Open,
    HalfOpen,
}

/// Per-service mutable state.
#[derive(Debug, Clone)]
pub struct ServiceState {
    /// Structural integrity `[0,1]`: 1 = sound, 0 = collapsed.
    pub health: f32,
    /// Backlog of unserved requests.
    pub queue: f32,
    /// Current one-hop latency in ticks.
    pub latency: f32,
    /// Requests offered this tick.
    pub demand: f32,
    /// Requests actually served this tick.
    pub admitted: f32,
    /// Exponential error rate `[0,1+]`.
    pub error_rate: f32,
    /// Retry debt carried into the next tick.
    pub retry_carry: f32,
    pub breaker: Breaker,
    pub breaker_timer: u16,
    pub breaker_ticks: u16,
    pub replicas: u16,
    pub version: u16,
    pub bad_deploy: bool,
    /// Injected raw fault magnitude `[0,1]`.
    pub fault: f32,
    /// Severed from the graph (no traffic reaches it).
    pub isolated: bool,
    /// Fraction of offered load shed by the operator.
    pub shed: f32,
    /// Temporary capacity boost from a reroute decision.
    pub reroute_boost: f32,
    /// Upstream stress inherited from dependencies, `[0,1]`.
    pub dep_stress: f32,
    /// Last frame this service emitted a semantic event.
    pub last_event_frame: u32,
    /// Monotonic count of breaker trips (a churn signal for the visuals).
    pub trips: u32,
}

impl ServiceState {
    fn new(def: &crate::sim::fixture::ServiceDef) -> Self {
        ServiceState {
            health: 1.0,
            queue: 0.0,
            latency: def.base_latency,
            demand: 0.0,
            admitted: 0.0,
            error_rate: 0.0,
            retry_carry: 0.0,
            breaker: Breaker::Closed,
            breaker_timer: 0,
            breaker_ticks: 0,
            replicas: def.replicas,
            version: 1,
            bad_deploy: false,
            fault: 0.0,
            isolated: false,
            shed: 0.0,
            reroute_boost: 0.0,
            dep_stress: 0.0,
            last_event_frame: 0,
            trips: 0,
        }
    }

    /// Effective capacity per tick.
    pub fn capacity(&self, def: &crate::sim::fixture::ServiceDef) -> f32 {
        if self.isolated {
            return 0.0;
        }
        let deploy = if self.bad_deploy { 0.45 } else { 1.0 };
        let faulted = 1.0 - 0.75 * self.fault;
        let boost = 1.0 + self.reroute_boost;
        (self.replicas as f32) * def.capacity * self.health.max(0.02) * deploy * faulted * boost
    }

    pub fn load_ratio(&self, def: &crate::sim::fixture::ServiceDef) -> f32 {
        (self.queue / def.queue_cap).clamp(0.0, 4.0)
    }

    /// A scalar "is this service in trouble" in `[0,1]` for visuals and phase logic.
    pub fn distress(&self, def: &crate::sim::fixture::ServiceDef) -> f32 {
        let load = self.load_ratio(def).min(1.0);
        let unhealthy = 1.0 - self.health;
        let err = self.error_rate.min(1.0);
        (load.max(unhealthy).max(err)).clamp(0.0, 1.0)
    }
}

/// System-wide aggregate metrics for one tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct Metrics {
    pub frame: u32,
    pub mean_health: f32,
    pub min_health: f32,
    pub frac_critical: f32,
    pub frac_overloaded: f32,
    pub open_breakers: u32,
    pub total_demand: f32,
    pub total_admitted: f32,
    pub total_queue: f32,
    pub mean_latency: f32,
    pub max_latency: f32,
    pub error_rate: f32,
    pub retry_debt: f32,
}

/// The simulation.
pub struct Engine {
    pub fixture: Fixture,
    pub states: Vec<ServiceState>,
    pub frame: u32,
    /// Edges that carry live traffic this tick: `(caller, callee, weight, flow)`.
    pub links: Vec<LinkFlow>,
    pub metrics: Metrics,
    order: Vec<ServiceId>,
    callers: Vec<Vec<ServiceId>>,
    pub rng_calls: u64,
}

/// A dependency edge annotated with this tick's realized flow.
#[derive(Debug, Clone, Copy)]
pub struct LinkFlow {
    pub from: ServiceId,
    pub to: ServiceId,
    /// Fraction of the caller's admitted load offered to this dependency.
    pub weight: f32,
    /// Realized requests/tick travelling along the edge.
    pub flow: f32,
    /// Inherited error probability on the edge.
    pub error: f32,
}

impl Engine {
    pub fn new(fixture: Fixture) -> Engine {
        let states = fixture.services.iter().map(ServiceState::new).collect();
        let order = fixture.topo_order();
        let mut callers: Vec<Vec<ServiceId>> = vec![Vec::new(); fixture.len()];
        for s in &fixture.services {
            for &d in &s.deps {
                callers[d as usize].push(s.id);
            }
        }
        Engine {
            fixture,
            states,
            frame: 0,
            links: Vec::new(),
            metrics: Metrics::default(),
            order,
            callers,
            rng_calls: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.fixture.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fixture.is_empty()
    }

    /// The service with the most direct dependents (ties broken by lowest id).
    ///
    /// This is the keystone the scripted incident targets; the interactive host
    /// starts focused here so the first operator fault exercises the cascade.
    pub fn most_depended_on(&self) -> ServiceId {
        let mut best = 0u16;
        let mut best_n = 0usize;
        for s in &self.fixture.services {
            let n = self
                .fixture
                .services
                .iter()
                .filter(|x| x.deps.contains(&s.id))
                .count();
            if n > best_n {
                best_n = n;
                best = s.id;
            }
        }
        best
    }

    /// How many services transitively depend on `id` (excluding `id` itself).
    ///
    /// This is the blast radius of a fault placed on `id`: a leaf with zero
    /// downstream dependents degrades in isolation and looks inert, while a
    /// keystone with many downstream dependents cascades. The operator UI uses it
    /// so a fault always reports its own fan-out.
    pub fn downstream_count(&self, id: ServiceId) -> usize {
        let mut seen = vec![false; self.fixture.len()];
        seen[id as usize] = true;
        let mut stack = vec![id];
        let mut count = 0usize;
        while let Some(cur) = stack.pop() {
            for &caller in &self.callers[cur as usize] {
                if !seen[caller as usize] {
                    seen[caller as usize] = true;
                    count += 1;
                    stack.push(caller);
                }
            }
        }
        count
    }

    /// Apply an operator action. Called at the frame the action was issued.
    pub fn apply(&mut self, action: &Action) {
        let t = action.target as usize;
        match action.kind {
            ActionKind::InjectFault => {
                let mag = (action.magnitude.clamp(0.1, 1.0)).min(1.0);
                let s = &mut self.states[t];
                s.fault = s.fault.max(mag);
                s.bad_deploy = true;
                s.version += 1;
                s.last_event_frame = self.frame;
            }
            ActionKind::Rollback => {
                let s = &mut self.states[t];
                s.bad_deploy = false;
                s.fault = (s.fault - 0.6).max(0.0);
                s.version += 1;
                s.last_event_frame = self.frame;
            }
            ActionKind::Isolate => {
                self.states[t].isolated = true;
                self.states[t].last_event_frame = self.frame;
            }
            ActionKind::Reinstate => {
                self.states[t].isolated = false;
                self.states[t].last_event_frame = self.frame;
            }
            ActionKind::Reroute => {
                self.states[t].shed = self.states[t].shed.max(0.7);
                // Boost up to two healthy same-cluster peers.
                let cluster = self.fixture.services[t].cluster;
                let peers: Vec<ServiceId> = self
                    .fixture
                    .services
                    .iter()
                    .filter(|s| s.cluster == cluster && s.id != action.target)
                    .map(|s| s.id)
                    .collect();
                for &p in peers.iter().take(2) {
                    self.states[p as usize].reroute_boost = 0.4;
                }
            }
            ActionKind::ShedLoad => {
                let pct = action.magnitude.clamp(0.0, 1.0);
                self.states[t].shed = pct;
            }
            ActionKind::ClearFault => {
                self.states[t].fault = 0.0;
                self.states[t].bad_deploy = false;
            }
            ActionKind::Acknowledge | ActionKind::Annotate | ActionKind::SetPhase => {
                // Journal/incident-level actions; no direct state mutation.
            }
        }
    }

    /// Advance exactly one logical tick.
    pub fn step(&mut self) {
        let frame = self.frame;
        let n = self.fixture.len();

        // Decay temporary reroute boosts and shed slowly back toward the operator intent.
        for s in &mut self.states {
            s.reroute_boost = (s.reroute_boost - 0.004).max(0.0);
        }

        let mut demand = vec![0.0f32; n];
        let mut admitted = vec![0.0f32; n];
        let mut new_links: Vec<LinkFlow> = Vec::with_capacity(self.links.len());

        // Pass 1 (callers before callees): demand, admission, queue, breaker.
        for &id in &self.order {
            let i = id as usize;
            let def = &self.fixture.services[i];
            let st = &self.states[i];

            let mut d = def.entry_rate;
            // Incoming load from callers already processed this tick.
            for &c in &self.callers[i] {
                let cdef = &self.fixture.services[c as usize];
                let cw = 1.0 / cdef.deps.len().max(1) as f32;
                d += admitted[c as usize] * cw;
            }
            d += self.states[i].retry_carry;
            // Deterministic per-(frame, service) traffic jitter: order-independent.
            let jitter = 1.0 + (Rng::hash01(frame as u64, i as u64) - 0.5) * 0.06;
            d *= jitter;

            let capped = st.capacity(def);
            let offered = (d * (1.0 - st.shed)).max(0.0);
            let served = if st.breaker == Breaker::Open {
                0.0
            } else if st.breaker == Breaker::HalfOpen {
                offered.min(capped * 0.15)
            } else {
                offered.min(capped)
            };

            demand[i] = d;
            admitted[i] = served;

            // Queue accumulates the shortfall; overflow times out and becomes retry
            // debt. A service whose breaker is *open* rejects immediately instead:
            // no queue forms and, crucially, a rejected request is not retried, so
            // an open breaker cannot feed a retry runaway. This is what lets the
            // system actually recover once the fault is removed.
            let breaker_open = st.breaker == Breaker::Open;
            let overflow = (offered - served).max(0.0);
            let mut q = st.queue * 0.86;
            let mut errs = 0.0f32;
            let mut retry = self.states[i].retry_carry * 0.2;
            if breaker_open {
                // A breaker rejecting requests is the breaker working, not evidence
                // of dependency failure. Counting these rejections as errors would
                // pin the error rate at 1.0 and prevent the breaker from ever
                // observing that the fault below it has cleared.
            } else {
                q += overflow;
                if q > def.queue_cap {
                    let over = q - def.queue_cap;
                    q = def.queue_cap;
                    errs += over;
                    retry += over * def.retry_factor;
                }
            }
            let own_err = if offered > 1e-3 {
                (errs / offered).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let smoothed = self.states[i].error_rate * 0.7 + own_err * 0.3;

            let s = &mut self.states[i];
            s.demand = d;
            s.admitted = served;
            s.queue = q;
            s.retry_carry = retry.min(def.queue_cap * 0.5);
            s.error_rate = smoothed;
        }

        // Pass 2 (reverse order, callees before callers): inherited dependency stress.
        for &id in self.order.iter().rev() {
            let i = id as usize;
            let def = &self.fixture.services[i];
            let mut stress = 0.0f32;
            for &d in &def.deps {
                let ds = &self.states[d as usize];
                let s = if ds.isolated {
                    1.0
                } else {
                    (1.0 - ds.health)
                        .max(ds.load_ratio(&self.fixture.services[d as usize]).min(1.0))
                };
                stress = stress.max(s);
            }
            self.states[i].dep_stress = stress;
        }

        // Health integration and latency, then emit link flows.
        for i in 0..n {
            let def = &self.fixture.services[i];
            let s = &mut self.states[i];
            let load = s.load_ratio(def).min(1.0);
            let stress = load.max(s.dep_stress).max(s.error_rate.min(1.0));
            // Health is a *recoverable* relaxation, not accumulated damage. Stress
            // below the knee (0.7) leaves a service fully sound; above it, integrity
            // eases toward a stress-implied target. Remove the fault and shed load,
            // and the same geometry heals back — which is what the recovery visual
            // and the score depend on.
            let excess = ((stress - 0.7) / 0.3).clamp(0.0, 1.0);
            let base_target = 1.0 - 0.9 * excess;
            // A live fault / bad deployment caps integrity even without overload.
            let ceiling = (1.0 - 0.6 * s.fault - if s.bad_deploy { 0.25 } else { 0.0 }).max(0.04);
            let target = base_target.min(ceiling).clamp(0.02, 1.0);
            let rate = if target < s.health { 0.30 } else { 0.05 };
            s.health = (s.health + (target - s.health) * rate).clamp(0.02, 1.0);

            let stretch = 1.0 + 3.0 * s.queue / def.queue_cap + 2.0 * (1.0 - s.health);
            s.latency = def.base_latency * stretch + s.dep_stress * 2.0;

            // Circuit breaker transition.
            if s.error_rate > 0.45 {
                s.breaker_ticks = s.breaker_ticks.saturating_add(1);
            } else {
                s.breaker_ticks = 0;
            }
            match s.breaker {
                Breaker::Closed => {
                    if s.breaker_ticks > 4 {
                        s.breaker = Breaker::Open;
                        s.breaker_timer = 24;
                        s.trips += 1;
                        s.last_event_frame = self.frame;
                    }
                }
                Breaker::Open => {
                    if s.breaker_timer > 0 {
                        s.breaker_timer -= 1;
                    } else {
                        // Enter a real half-open *trial window*: the breaker must have
                        // several ticks to observe whether the error rate decays before
                        // it can close. Without this the half-open state is one tick long
                        // and a service can never recover.
                        s.breaker = Breaker::HalfOpen;
                        s.breaker_timer = 12;
                    }
                }
                Breaker::HalfOpen => {
                    if s.breaker_timer > 0 {
                        s.breaker_timer -= 1;
                    }
                    if s.error_rate < 0.2 && stress < 0.5 {
                        s.breaker = Breaker::Closed;
                        s.breaker_ticks = 0;
                    } else if s.breaker_timer == 0 {
                        s.breaker = Breaker::Open;
                        s.breaker_timer = 18;
                    }
                }
            }
        }

        // Realized link flows for the renderer and the cascade view.
        for i in 0..n {
            let def = &self.fixture.services[i];
            let from_state = &self.states[i];
            for &d in &def.deps {
                let ds = &self.states[d as usize];
                let weight = 1.0 / def.deps.len().max(1) as f32;
                let flow = if ds.isolated || from_state.isolated {
                    0.0
                } else {
                    from_state.admitted * weight
                };
                let error = if ds.isolated {
                    1.0
                } else {
                    (0.5 * ds.error_rate + 0.5 * (1.0 - ds.health)).clamp(0.0, 1.0)
                };
                new_links.push(LinkFlow {
                    from: def.id,
                    to: d,
                    weight,
                    flow,
                    error,
                });
            }
        }
        self.links = new_links;

        // Aggregate metrics.
        let mut m = Metrics {
            frame,
            min_health: 1.0,
            ..Metrics::default()
        };
        let inv = 1.0 / n.max(1) as f32;
        let mut crit = 0u32;
        let mut over = 0u32;
        let mut open = 0u32;
        for i in 0..n {
            let def = &self.fixture.services[i];
            let s = &self.states[i];
            m.mean_health += s.health;
            m.min_health = m.min_health.min(s.health);
            if s.health < 0.5 {
                crit += 1;
            }
            if s.load_ratio(def) > 0.8 {
                over += 1;
            }
            if s.breaker == Breaker::Open {
                open += 1;
            }
            m.total_demand += s.demand;
            m.total_admitted += s.admitted;
            m.total_queue += s.queue;
            m.mean_latency += s.latency;
            m.max_latency = m.max_latency.max(s.latency);
            m.error_rate += s.error_rate;
            m.retry_debt += s.retry_carry;
        }
        m.mean_health *= inv;
        m.mean_latency *= inv;
        m.error_rate *= inv;
        m.frac_critical = crit as f32 * inv;
        m.frac_overloaded = over as f32 * inv;
        m.open_breakers = open;
        self.metrics = m;

        self.frame = self.frame.wrapping_add(1);
    }

    /// Canonical semantic digest: quantized per-service state, order-independent
    /// within the fixed fixture order. Used to prove replay equivalence.
    pub fn semantic_digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mix = |h: &mut u64, v: u64| {
            *h ^= v
                .wrapping_add(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(*h << 6)
                .wrapping_add(*h >> 2);
        };
        for (i, s) in self.states.iter().enumerate() {
            let q = |x: f32| ((x * 1000.0).round() as i64) as u64;
            mix(&mut h, i as u64);
            mix(&mut h, q(s.health));
            mix(&mut h, q(s.queue));
            mix(&mut h, q(s.latency));
            mix(&mut h, q(s.error_rate));
            mix(&mut h, q(s.shed));
            mix(&mut h, s.breaker as u64);
            mix(&mut h, s.version as u64);
            mix(&mut h, s.bad_deploy as u64);
            mix(&mut h, s.isolated as u64);
            mix(&mut h, q(s.fault));
        }
        h
    }

    pub fn reset(&mut self) {
        for (i, s) in self.states.iter_mut().enumerate() {
            *s = ServiceState::new(&self.fixture.services[i]);
        }
        self.frame = 0;
        self.links.clear();
        self.metrics = Metrics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, ActionKind};
    use crate::sim::fixture::Fixture;

    /// At rest the provisioned system must be stable: no critical services, no
    /// open breakers, no queues, and full integrity. This is the calibration the
    /// scripted incident depends on — without it, "a minor fault" would be
    /// indistinguishable from the baseline.
    #[test]
    fn resting_system_is_stable() {
        let mut e = Engine::new(Fixture::cathedral());
        for _ in 0..200 {
            e.step();
            assert_eq!(e.metrics.frac_critical, 0.0, "baseline critical services");
            assert_eq!(e.metrics.open_breakers, 0, "baseline open breakers");
            assert!(
                e.metrics.mean_health > 0.999,
                "baseline mean health {}",
                e.metrics.mean_health
            );
        }
    }

    /// The scripted incident must cascade and then recover fully: after the fault
    /// is injected the critical fraction rises sharply and breakers open; after
    /// rollback + reroute the system returns to zero critical, zero breakers and
    /// full integrity. This is the core WTF scenario contract.
    #[test]
    fn fault_cascades_then_recovers() {
        let mut e = Engine::new(Fixture::cathedral());
        let target: ServiceId = 138;
        let mut peak_critical = 0.0f32;
        let mut peak_breakers = 0u32;
        for f in 0..900u32 {
            match f {
                59 => e.apply(&Action::new(
                    f,
                    1,
                    ActionKind::InjectFault,
                    target,
                    0.6,
                    "x",
                )),
                300 => e.apply(&Action::new(
                    f,
                    2,
                    ActionKind::Acknowledge,
                    target,
                    0.6,
                    "x",
                )),
                340 => e.apply(&Action::new(f, 3, ActionKind::Rollback, target, 0.6, "x")),
                360 => e.apply(&Action::new(f, 4, ActionKind::Reroute, target, 0.6, "x")),
                _ => {}
            }
            e.step();
            if (100..=400).contains(&f) {
                peak_critical = peak_critical.max(e.metrics.frac_critical);
                peak_breakers = peak_breakers.max(e.metrics.open_breakers);
            }
        }
        assert!(
            peak_critical > 0.08,
            "cascade never formed: {peak_critical}"
        );
        assert!(peak_breakers >= 1, "no breaker tripped during cascade");
        // Recovery.
        assert_eq!(e.metrics.frac_critical, 0.0, "did not recover: critical");
        assert_eq!(e.metrics.open_breakers, 0, "did not recover: breakers");
        assert!(e.metrics.mean_health > 0.999, "did not recover: health");
    }

    /// The blast radius of a fault is what makes the operator affordance honest:
    /// the keystone cascades, a leaf only degrades itself. This is the property the
    /// UI now surfaces, so it is pinned here.
    #[test]
    fn downstream_fanout_identifies_the_keystone() {
        let e = Engine::new(Fixture::cathedral());
        // The scripted incident targets #138 because it is the most directly
        // depended-upon service; a fault there must have a real blast radius.
        assert_eq!(e.most_depended_on(), 138);
        assert!(e.downstream_count(138) >= 20, "keystone fan-out too small");
        // #0 is a leaf with no dependents: faulting it can only degrade itself.
        assert_eq!(e.downstream_count(0), 0);
    }

    /// Determinism: the same fixture stepped twice yields identical state digests.
    #[test]
    fn stepping_is_deterministic() {
        let mut a = Engine::new(Fixture::cathedral());
        let mut b = Engine::new(Fixture::cathedral());
        for f in 0..300u32 {
            if f == 59 {
                a.apply(&Action::new(f, 1, ActionKind::InjectFault, 138, 0.6, "x"));
                b.apply(&Action::new(f, 1, ActionKind::InjectFault, 138, 0.6, "x"));
            }
            a.step();
            b.step();
        }
        assert_eq!(a.semantic_digest(), b.semantic_digest());
    }
}
