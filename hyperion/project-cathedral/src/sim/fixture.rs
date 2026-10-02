//! The deterministic fictional service topology — Project Cathedral's "city plan".
//!
//! This is a **fictional stress domain**, not a model of any production system. The
//! graph shape, capacities and failure couplings were chosen so that a single small
//! perturbation can propagate, not to represent real distributed-system statistics.
//!
//! The fixture is generated deterministically from a fixed seed, so the topology is
//! stable across runs while still being large (~256 services, ~430 dependency edges).

use crate::rng::Rng;

pub type ServiceId = u16;

/// One service in the fictional topology.
#[derive(Debug, Clone)]
pub struct ServiceDef {
    pub id: ServiceId,
    /// Architectural district (visual cluster).
    pub cluster: u8,
    /// Dependency layer: 0 = edge/entry, 5 = deepest shared resource.
    pub tier: u8,
    /// Services this one calls (outbound dependencies). Same cluster preferred.
    pub deps: Vec<ServiceId>,
    pub replicas: u16,
    /// Requests per tick that one healthy replica can serve.
    pub capacity: f32,
    /// Nominal one-hop latency in ticks.
    pub base_latency: f32,
    /// Queue length at which the service starts shedding/timeout-failing.
    pub queue_cap: f32,
    /// Retry amplification: timed-out requests re-enter at this multiple.
    pub retry_factor: f32,
    /// External entry demand (requests/tick) placed directly on this service.
    pub entry_rate: f32,
    pub name: String,
}

/// The whole fictional system.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: String,
    pub seed: u64,
    pub services: Vec<ServiceDef>,
    pub clusters: Vec<String>,
}

#[allow(clippy::unusual_byte_groupings)] // grouped as a readable seed, "cathedral base 2026-04"
pub const DEFAULT_FIXTURE_SEED: u64 = 0xCA7ED_BA5E_2026_04;

const CLUSTERS: [&str; 9] = [
    "NAVE",
    "TRANSEPT",
    "APSE",
    "CRYPT",
    "BELLTOWER",
    "CLOISTER",
    "CHAPTER",
    "ROSE",
    "VAULT",
];

const ROLES: [&str; 12] = [
    "gate", "relay", "index", "ledger", "cache", "queue", "auth", "render", "shard", "mail",
    "audit", "clock",
];

impl Fixture {
    /// Build the default named fixture deterministically.
    pub fn cathedral() -> Fixture {
        Fixture::generate("cathedral", DEFAULT_FIXTURE_SEED)
    }

    /// Generate a fixture with `count` services (rounded per cluster) from `seed`.
    pub fn generate(name: &str, seed: u64) -> Fixture {
        let mut rng = Rng::new(seed);
        let n_clusters = CLUSTERS.len();
        let mut by_cluster: Vec<Vec<ServiceId>> = vec![Vec::new(); n_clusters];
        let mut services: Vec<ServiceDef> = Vec::new();

        // Target ~256 services: 24..=34 per district.
        for (c, chunk) in CLUSTERS.iter().enumerate() {
            let count = 24 + rng.range(0, 11); // 24..=34
            for i in 0..count {
                let id = services.len() as ServiceId;
                let role = ROLES[(id as usize + c) % ROLES.len()];
                let name = format!("{}-{}{:02}", chunk, role, i);
                // Entry tier 0 is rare per cluster; deeper tiers get more nodes.
                let tier: u8 = match rng.range(0, 100) {
                    0..=14 => 0,
                    15..=39 => 1,
                    40..=64 => 2,
                    65..=84 => 3,
                    85..=95 => 4,
                    _ => 5,
                };
                let replicas = rng.range(1, 4) as u16;
                let capacity = 30.0 + rng.f32() * 90.0;
                let base_latency = 1.0 + tier as f32 * 0.4 + rng.f32() * 0.6;
                let queue_cap = capacity * (2.5 + rng.f32() * 2.0);
                let retry_factor = 1.15 + rng.f32() * 0.35;
                // Tier-0 services are the request entry points; the rest carry a trickle
                // of background work so nothing is inert.
                let entry_rate = if tier == 0 {
                    capacity * replicas as f32 * (0.30 + rng.f32() * 0.12)
                } else {
                    capacity * replicas as f32 * 0.04
                };
                services.push(ServiceDef {
                    id,
                    cluster: c as u8,
                    tier,
                    deps: Vec::new(),
                    replicas,
                    capacity,
                    base_latency,
                    queue_cap,
                    retry_factor,
                    entry_rate,
                    name,
                });
                by_cluster[c].push(id);
            }
        }

        // Wire dependencies: each service calls 1..=3 services in a strictly deeper tier,
        // 72% of the time from its own district.
        let all: Vec<ServiceId> = services.iter().map(|s| s.id).collect();
        let mut deeper: Vec<Vec<ServiceId>> = vec![Vec::new(); 6];
        for s in &services {
            deeper[s.tier as usize].push(s.id);
        }
        for idx in 0..services.len() {
            let (tier, cluster) = (services[idx].tier, services[idx].cluster);
            if tier >= 5 {
                continue;
            }
            let n_deps = rng.range(1, 4);
            let mut deps: Vec<ServiceId> = Vec::new();
            for _ in 0..n_deps {
                let target_tier = tier + 1 + rng.range(0, (5 - tier).max(1) as u32) as u8;
                let target_tier = target_tier.min(5);
                let same: Vec<ServiceId> = deeper[target_tier as usize]
                    .iter()
                    .copied()
                    .filter(|&d| services[d as usize].cluster == cluster)
                    .collect();
                let pool: &Vec<ServiceId> = if !same.is_empty() && rng.range(0, 100) < 72 {
                    &same
                } else {
                    &deeper[target_tier as usize]
                };
                if pool.is_empty() {
                    continue;
                }
                let pick = pool[rng.range(0, pool.len() as u32) as usize];
                if pick != services[idx].id && !deps.contains(&pick) {
                    deps.push(pick);
                }
            }
            deps.sort_unstable();
            deps.dedup();
            services[idx].deps = deps;
        }
        let _ = all;

        // ── Provision capacity for the expected resting load ──────────────────
        // Propagate the entry demand through the DAG in topological (callers-first)
        // order, assuming nothing saturates, then give each service enough
        // per-replica capacity to serve that expected load at ~60% utilization.
        // This keeps the *resting* system stable, so the scripted fault — not a
        // permanent baseline overload — is what causes the incident. The heavy
        // fan-in that makes a single perturbation propagate is preserved.
        let n = services.len();
        let mut callers: Vec<Vec<ServiceId>> = vec![Vec::new(); n];
        for s in &services {
            for &d in &s.deps {
                callers[d as usize].push(s.id);
            }
        }
        let mut order: Vec<ServiceId> = (0..n as ServiceId).collect();
        order.sort_by_key(|&id| services[id as usize].tier);
        let mut expected: Vec<f32> = vec![0.0; n];
        for &id in &order {
            let i = id as usize;
            let mut d = services[i].entry_rate;
            for &c in &callers[i] {
                let ci = c as usize;
                let n_deps = services[ci].deps.len().max(1) as f32;
                d += expected[ci] / n_deps;
            }
            expected[i] = d;
        }
        for i in 0..n {
            if expected[i] > 0.0 {
                let per_replica = expected[i] / (services[i].replicas as f32 * 0.6);
                services[i].capacity = services[i].capacity.max(per_replica);
            }
            // Queue capacity is a few ticks of the service's *total* capacity.
            let total = services[i].replicas as f32 * services[i].capacity;
            services[i].queue_cap = total * (1.4 + rng.f32() * 1.6);
        }

        Fixture {
            name: name.to_string(),
            seed,
            services,
            clusters: CLUSTERS.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.services.len()
    }

    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    pub fn service(&self, id: ServiceId) -> &ServiceDef {
        &self.services[id as usize]
    }

    /// All dependency edges `(caller, callee)`.
    pub fn edges(&self) -> Vec<(ServiceId, ServiceId)> {
        let mut out = Vec::new();
        for s in &self.services {
            for &d in &s.deps {
                out.push((s.id, d));
            }
        }
        out
    }

    /// An order in which callers precede callees (deps form a DAG by construction).
    pub fn topo_order(&self) -> Vec<ServiceId> {
        let mut order: Vec<ServiceId> = (0..self.services.len() as ServiceId).collect();
        // Tier is a topological potential: a dep always has a strictly greater tier.
        order.sort_by_key(|&id| self.services[id as usize].tier);
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_stable_and_large() {
        let f = Fixture::cathedral();
        assert!((100..=500).contains(&f.len()), "services: {}", f.len());
        let edges = f.edges();
        assert!(edges.len() > 200, "edges: {}", edges.len());
        // Determinism.
        let g = Fixture::cathedral();
        assert_eq!(f.len(), g.len());
        assert_eq!(f.edges(), g.edges());
    }

    #[test]
    fn dependencies_are_acyclic_and_deeper() {
        let f = Fixture::cathedral();
        for s in &f.services {
            for &d in &s.deps {
                assert!(
                    f.service(d).tier > s.tier,
                    "dep {} (tier {}) not deeper than {} (tier {})",
                    d,
                    f.service(d).tier,
                    s.id,
                    s.tier
                );
                assert_ne!(d, s.id);
            }
        }
    }

    #[test]
    fn topo_order_places_callers_first() {
        let f = Fixture::cathedral();
        let order = f.topo_order();
        let mut pos = vec![0usize; f.len()];
        for (i, &id) in order.iter().enumerate() {
            pos[id as usize] = i;
        }
        for s in &f.services {
            for &d in &s.deps {
                assert!(pos[s.id as usize] < pos[d as usize]);
            }
        }
    }
}
