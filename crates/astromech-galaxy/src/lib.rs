//! # astromech-galaxy
//!
//! Where things are and how many jumps away. A local SQLite star map, seeded from a bundled
//! preload (500 ly around Sol, `scripts/gen-starmap.py`) and filled in from EDSM as the
//! commander travels; live faction states and Powerplay targets from Spansh's system search;
//! jump counts by A* over the known stars at the ship's range; raw-material sites (crystalline
//! shards, brain trees, crashed ships, geological sites, the known farms) from Spansh's bodies
//! search.

pub mod filters;
pub mod map;
pub mod module_list;
pub mod outfitting;
pub mod powerplay;
pub mod remote;
pub mod sites;
pub mod stations;
pub mod store;
pub mod visits;

pub use filters::{Facts, Filters, Pad};
pub use map::{estimate, Hop, StarMap};
pub use powerplay::{Target, TargetHit, Targets};
pub use remote::{query, Faction, Hit, Query, QUERIES};
pub use sites::{site_kind, SiteHit, SiteKind, Sites, SITE_KINDS};
pub use store::{dist, Sphere, Star, Store};
pub use visits::{visits_from_journal_dir, visits_from_lines, Visit};

use anyhow::Result;
use astromech_journal::JournalEvent;
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

/// Radius fetched from EDSM when the commander is outside covered space.
pub const FILL_RADIUS: f64 = 50.0;
/// How long a faction-state search is reused.
pub const STATE_TTL_S: i64 = 3600;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Here {
    pub system: String,
    pub id64: Option<i64>,
    pub pos: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NearbyHit {
    pub name: String,
    pub id64: i64,
    #[serde(flatten)]
    pub hop: Hop,
    pub population: i64,
    pub updated_at: Option<String>,
    /// Only the factions that match the query.
    pub factions: Vec<Faction>,
    /// The factions and `updated_at` are from the commander's own visit (journal), newer than
    /// Spansh's.
    pub seen_on_visit: bool,
    /// [`hge_score`] × 100 (100 = a billion people in this system), for lookups ranked by population.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Nearby {
    pub query: Query,
    pub here: Here,
    pub range: f64,
    /// When the search was made (cached up to [`STATE_TTL_S`]).
    pub fetched: String,
    pub hits: Vec<NearbyHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// A station from a station lookup, with jumps at the search's range.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StationNearby {
    #[serde(flatten)]
    pub station: stations::StationHit,
    pub jumps: u32,
    /// false: `jumps` is a straight-line estimate.
    pub exact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StationsNearby {
    /// What was looked for ("Interstellar Factors").
    pub want: String,
    pub here: Here,
    pub range: f64,
    /// The filters applied (the pad resolved for the current ship).
    pub filters: Filters,
    /// When Spansh answered (may be a cached answer).
    pub fetched: String,
    /// Nearest first.
    pub hits: Vec<StationNearby>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// A station selling some of a module list, with jumps at the search's range.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutfitterNearby {
    #[serde(flatten)]
    pub station: module_list::Outfitter,
    pub jumps: u32,
    /// false: `jumps` is a straight-line estimate.
    pub exact: bool,
}

/// Where to buy a list of modules.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModulesNearby {
    pub here: Here,
    pub range: f64,
    /// The filters applied (the pad resolved for the current ship).
    pub filters: Filters,
    /// When Spansh answered (the oldest answer used; may be cached).
    pub fetched: String,
    /// Most of the list first, then nearest.
    pub stations: Vec<OutfitterNearby>,
    /// Listed modules no station found sells.
    pub unavailable: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

pub struct Galaxy {
    store: Mutex<Store>,
    remote: astromech_data::Client,
    map: RwLock<Option<Arc<StarMap>>>,
    /// Last failed EDSM fill-in: don't retry for [`FILL_RETRY_S`].
    fill_failed: Mutex<Option<i64>>,
}

/// Population at which High Grade Emissions spawn reliably.
pub const HGE_POPULATION: i64 = 1_000_000_000;

/// How good a system is for High Grade Emissions, 0 to about 1.2: population over distance.
/// A system over a billion people scores 1 to 1.5 for its population (more is better, up to
/// ten billion); under a billion it scores at most 0.25, so it ranks well below. Distance
/// halves the score at 50 ly. A 2-billion system 30 ly away (0.72) beats a 200-million system
/// 5 ly away (0.05).
pub fn hge_score(population: i64, ly: f64) -> f64 {
    let pop = if population >= HGE_POPULATION {
        1.0 + 0.5 * (population as f64 / HGE_POPULATION as f64).log10().min(1.0)
    } else {
        0.25 * population.max(0) as f64 / HGE_POPULATION as f64
    };
    pop / (1.0 + ly.max(0.0) / 50.0)
}

/// After an EDSM failure, wait this long before trying again.
pub const FILL_RETRY_S: i64 = 600;

fn now_s() -> i64 {
    chrono::Utc::now().timestamp()
}

pub(crate) fn iso(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0).map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string()).unwrap_or_default()
}

impl Galaxy {
    /// Open (or create) the store at `path`, seeding it from `preload` once.
    pub fn open(path: &Path, preload: Option<&Path>) -> Result<Self> {
        let mut s = Store::open(path)?;
        if let Some(p) = preload {
            s.seed_from(p)?;
        }
        Ok(Self::with_store(s))
    }

    pub fn with_store(s: Store) -> Self {
        Self::with_remote(s, astromech_data::Client::new(astromech_data::Cache::in_memory().expect("in-memory SQLite")))
    }

    /// `remote` answers the Spansh and EDSM requests (tests pass a recorded transport).
    pub fn with_remote(s: Store, remote: astromech_data::Client) -> Self {
        Galaxy { store: Mutex::new(s), remote, map: RwLock::new(None), fill_failed: Mutex::new(None) }
    }

    /// The remote-data client this galaxy's lookups go through (other lookups share it, and
    /// its cache).
    pub fn remote(&self) -> &astromech_data::Client {
        &self.remote
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn map(&self) -> Result<Arc<StarMap>> {
        if let Some(m) = self.map.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return Ok(m.clone());
        }
        let (stars, cov) = {
            let s = self.store();
            (s.stars()?, s.coverage()?)
        };
        let m = Arc::new(StarMap::new(stars, cov));
        *self.map.write().unwrap_or_else(|e| e.into_inner()) = Some(m.clone());
        Ok(m)
    }

    fn invalidate(&self) {
        *self.map.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn summary(&self) -> Result<(i64, Option<String>)> {
        let s = self.store();
        Ok((s.count()?, s.meta("preload")?))
    }

    pub fn position(&self, system: &str) -> Result<Option<[f64; 3]>> {
        Ok(self.store().system(system)?.map(|x| x.2))
    }

    /// Up to a handful of system names resembling `prefix` (Spansh's fuzzy autocomplete), for
    /// typing a search origin.
    pub fn system_names(&self, prefix: &str) -> Result<Vec<String>> {
        Ok(self.remote.spansh_system_names(prefix)?.value.into_iter().map(|s| s.name).collect())
    }

    /// Where a typed system is: the local map first, else Spansh (exact name, any case).
    pub fn locate(&self, name: &str) -> Result<Here> {
        let name = name.trim();
        if let Some((id64, system, pos)) = self.store().system(name)? {
            return Ok(Here { system, id64: Some(id64), pos });
        }
        self.remote
            .spansh_system_names(name)?
            .value
            .into_iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .map(|s| Here { pos: s.pos(), system: s.name, id64: Some(s.id64) })
            .ok_or_else(|| anyhow::anyhow!("Spansh doesn't know a system called {name}."))
    }

    /// Fetch the stars around `p` from EDSM if `p` is outside covered space. Returns whether a
    /// fetch happened.
    pub fn ensure_covered(&self, p: [f64; 3]) -> Result<bool> {
        if self.map()?.covered(p) {
            return Ok(false);
        }
        let now = now_s();
        if self.fill_failed.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|t| now - t < FILL_RETRY_S) {
            return Ok(false);
        }
        let rows = match remote::edsm_sphere(&self.remote, p, FILL_RADIUS) {
            Ok(r) => r,
            Err(e) => {
                *self.fill_failed.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
                return Err(e);
            }
        };
        // An empty answer is not coverage: EDSM may simply not know the area yet.
        if rows.is_empty() {
            *self.fill_failed.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
            return Ok(false);
        }
        {
            let mut s = self.store();
            s.insert_systems(rows.iter().map(|(id, n, q)| (*id, n.as_str(), *q)))?;
            s.add_coverage(Sphere { center: p, radius: FILL_RADIUS }, "edsm", &iso(now))?;
        }
        self.invalidate();
        Ok(true)
    }

    pub fn hop(&self, a: [f64; 3], b: [f64; 3], range: f64) -> Result<Hop> {
        Ok(self.map()?.hop(a, b, range))
    }

    /// Remember the faction states seen on arrival (newest per system wins).
    pub fn record_visits(&self, visits: &[Visit]) -> Result<()> {
        let s = self.store();
        for v in visits {
            s.put_visit(v)?;
        }
        Ok(())
    }

    /// Replace a hit's factions and age with the commander's own visit when that is newer.
    fn refresh_from_visits(&self, hits: &mut [Hit]) -> std::collections::HashSet<i64> {
        let mut seen = std::collections::HashSet::new();
        let s = self.store();
        for h in hits {
            let Ok(Some(v)) = s.visit(h.id64) else { continue };
            let newer = match h.updated_at.as_deref().and_then(filters::parse_time) {
                Some(t) => filters::parse_time(&v.seen).is_some_and(|seen| seen > t),
                None => true,
            };
            if newer {
                h.minor_faction_presences = v.factions;
                h.updated_at = Some(v.seen);
                seen.insert(h.id64);
            }
        }
        seen
    }

    /// Systems near `here` matching `query_id` and `filters`, nearest first, with jump counts
    /// at `range`.
    pub fn nearby(&self, query_id: &str, here: &Here, range: f64, limit: usize, filters: &Filters) -> Result<Nearby> {
        let q = query(query_id).ok_or_else(|| anyhow::anyhow!("unknown lookup {query_id}"))?;
        // Fill-in is best effort: without it the jump counts are estimates.
        let _ = self.ensure_covered(here.pos);
        let now = now_s();
        let (fetched, body) = self.search_page(q, here, None, now)?;
        let mut hits = remote::parse_search(&body)?;
        if q.by_population {
            // Spansh's nearest matches can all be small (in the bubble the nearest 50 lie within
            // 20 ly): add the nearest systems over a billion people, then rank both together.
            let (_, big) = self.search_page(q, here, Some(HGE_POPULATION), now)?;
            for h in remote::parse_search(&big)? {
                if !hits.iter().any(|x| x.id64 == h.id64) {
                    hits.push(h);
                }
            }
        }
        let visited = self.refresh_from_visits(&mut hits);
        let hits = hits
            .into_iter()
            .filter(|h| filters.keeps(&Facts { updated_at: h.updated_at.as_deref(), ..Facts::default() }, now))
            .collect();
        Ok(self.rank(q, here, range, limit, hits, iso(fetched), &visited))
    }

    /// One Spansh systems search for `q` near `here` (cached for [`STATE_TTL_S`] per 10 ly cell):
    /// `(fetched, body)`.
    fn search_page(&self, q: &Query, here: &Here, min_population: Option<i64>, now: i64) -> Result<(i64, String)> {
        let key = format!(
            "spansh:{}{}:{:.0}:{:.0}:{:.0}",
            q.id,
            if min_population.is_some() { ":pop" } else { "" },
            (here.pos[0] / 10.0).round() + 0.0,
            (here.pos[1] / 10.0).round() + 0.0,
            (here.pos[2] / 10.0).round() + 0.0
        );
        if let Some(c) = self.store().cache_get(&key, now, STATE_TTL_S)? {
            return Ok(c);
        }
        let b = remote::spansh_search(&self.remote, q, here.pos, 50, min_population)?;
        remote::parse_search(&b)?;
        self.store().cache_put(&key, now, &b)?;
        Ok((now, b))
    }

    /// The nearest raw-material sites of `kind_id` ([`SITE_KINDS`]) to `here`, matching
    /// `filters` (data age and arrival distance), with jump counts at `range`.
    pub fn sites(&self, kind_id: &str, here: &Here, range: f64, limit: usize, filters: &Filters) -> Result<Sites> {
        let kind = site_kind(kind_id).ok_or_else(|| anyhow::anyhow!("unknown site kind {kind_id}"))?;
        // Fill-in is best effort: without it the jump counts are estimates.
        let _ = self.ensure_covered(here.pos);
        let now = now_s();
        let (source, fetched, found, warning) = match kind.search(here.pos, 30) {
            Some(search) => {
                let page = self.remote.spansh_bodies(&search)?;
                let found = sites::from_bodies(kind, &page.value.results);
                let warning = (!page.value.results.is_empty() && found.is_empty()).then(|| {
                    format!("Spansh returned bodies without {}; its search filter may have changed.", kind.noun)
                });
                ("Spansh", page.fetched_at, found, warning)
            }
            None => ("astromech", now, sites::from_farms(here.pos), None),
        };
        let found = sites::keep(found, filters, now);
        {
            let mut s = self.store();
            let _ = s.insert_systems(found.iter().map(|c| (c.id64, c.system.as_str(), c.pos)));
        }
        let map = self.map().ok();
        let mut hits: Vec<SiteHit> = found
            .into_iter()
            .take(limit.max(1) * 2)
            .map(|c| SiteHit {
                hop: hop_on(map.as_deref(), here.pos, c.pos, range),
                system: c.system,
                id64: c.id64,
                body: c.body,
                arrival_ls: c.arrival_ls,
                updated_at: c.updated_at,
                detail: c.detail,
                farm: c.farm,
            })
            .collect();
        hits.sort_by(|a, b| {
            a.hop
                .jumps
                .cmp(&b.hop.jumps)
                .then(a.hop.ly.total_cmp(&b.hop.ly))
                .then(a.arrival_ls.unwrap_or(f64::MAX).total_cmp(&b.arrival_ls.unwrap_or(f64::MAX)))
        });
        hits.truncate(limit.max(1));
        Ok(Sites { kind: kind.clone(), here: here.clone(), range, source, fetched: iso(fetched), hits, warning })
    }

    /// The nearest stations to `here` offering `want` and passing `filters`, nearest first,
    /// with jumps at `range`.
    pub fn stations(
        &self,
        want: &stations::Want,
        here: &Here,
        range: f64,
        limit: usize,
        filters: &Filters,
    ) -> Result<StationsNearby> {
        let _ = self.ensure_covered(here.pos);
        let reference = astromech_data::spansh::Reference::Coords(here.pos);
        let found = stations::find(&self.remote, want, reference, filters, limit, now_s())?;
        {
            let mut s = self.store();
            let _ = s.insert_systems(found.hits.iter().map(|h| (h.system_id64, h.system.as_str(), h.pos)));
        }
        let map = self.map().ok();
        let hits = found
            .hits
            .into_iter()
            .map(|h| {
                let hop = match &map {
                    Some(m) => m.hop(here.pos, h.pos, range),
                    None => Hop { ly: h.ly, jumps: estimate(h.ly, range), exact: false },
                };
                StationNearby { station: h, jumps: hop.jumps, exact: hop.exact }
            })
            .collect();
        Ok(StationsNearby {
            want: want.label(),
            here: here.clone(),
            range,
            filters: filters.clone(),
            fetched: iso(found.fetched_at),
            hits,
            warning: found.warning,
        })
    }

    /// Stations (up to `limit`) selling any of `wants` near `here`, most of the list first,
    /// then nearest, with jumps at `range` ([`module_list::find`]).
    pub fn module_list(
        &self,
        wants: &[outfitting::ModuleWant],
        here: &Here,
        range: f64,
        limit: usize,
        filters: &Filters,
    ) -> Result<ModulesNearby> {
        let _ = self.ensure_covered(here.pos);
        let reference = astromech_data::spansh::Reference::Coords(here.pos);
        let found = module_list::find(&self.remote, wants, reference, filters, limit, now_s())?;
        {
            let mut s = self.store();
            let _ = s.insert_systems(found.stations.iter().map(|h| (h.system_id64, h.system.as_str(), h.pos)));
        }
        let map = self.map().ok();
        let stations = found
            .stations
            .into_iter()
            .map(|h| {
                let hop = match &map {
                    Some(m) => m.hop(here.pos, h.pos, range),
                    None => Hop { ly: h.ly, jumps: estimate(h.ly, range), exact: false },
                };
                OutfitterNearby { station: h, jumps: hop.jumps, exact: hop.exact }
            })
            .collect();
        Ok(ModulesNearby {
            here: here.clone(),
            range,
            filters: filters.clone(),
            fetched: iso(found.fetched_at),
            stations,
            unavailable: found.unavailable,
            warning: found.warning,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn rank(
        &self,
        q: &Query,
        here: &Here,
        range: f64,
        limit: usize,
        hits: Vec<Hit>,
        fetched: String,
        visited: &std::collections::HashSet<i64>,
    ) -> Nearby {
        let n = hits.len();
        let matched: Vec<Hit> = hits.into_iter().filter(|h| q.matches(h)).collect();
        let warning = (n > 0 && matched.is_empty() && q.min_population == 0).then(|| {
            "Spansh returned systems without the requested state; its search filter may have changed.".to_string()
        });
        {
            let mut s = self.store();
            let _ = s.insert_systems(matched.iter().map(|h| (h.id64, h.name.as_str(), h.pos())));
        }
        let map = self.map().ok();
        let score = |h: &Hit| hge_score(h.population, dist(here.pos, h.pos()));
        let mut matched = matched;
        if q.by_population {
            // Best score first; only those shown need a jump count.
            matched.sort_by(|a, b| score(b).total_cmp(&score(a)));
        }
        let mut out: Vec<NearbyHit> = matched
            .into_iter()
            .take(limit.max(1) * 2)
            .map(|h| {
                let hop = hop_on(map.as_deref(), here.pos, h.pos(), range);
                NearbyHit {
                    name: h.name.clone(),
                    id64: h.id64,
                    hop,
                    population: h.population,
                    updated_at: h.updated_at.clone(),
                    factions: h.minor_faction_presences.iter().filter(|f| q.faction_matches(f)).cloned().collect(),
                    seen_on_visit: visited.contains(&h.id64),
                    score: q.by_population.then(|| (score(&h) * 100.0).round() as u32),
                }
            })
            .collect();
        if !q.by_population {
            out.sort_by(|a, b| a.hop.jumps.cmp(&b.hop.jumps).then(a.hop.ly.total_cmp(&b.hop.ly)));
        }
        out.truncate(limit.max(1));
        Nearby { query: q.clone(), here: here.clone(), range, fetched, hits: out, warning }
    }
}

/// Jumps on `map`, or the straight-line estimate without one.
fn hop_on(map: Option<&StarMap>, a: [f64; 3], b: [f64; 3], range: f64) -> Hop {
    match map {
        Some(m) => m.hop(a, b, range),
        None => {
            let ly = dist(a, b);
            Hop { ly, jumps: estimate(ly, range), exact: false }
        }
    }
}

const POSITION_EVENTS: &[&str] = &["\"Location\"", "\"FSDJump\"", "\"CarrierJump\""];

/// Newest `Location` / `FSDJump` / `CarrierJump` with `StarPos`, newest journal first.
pub fn here_from_journal_dir(dir: &Path) -> Option<Here> {
    let mut logs = astromech_journal::journal_logs(dir);
    logs.reverse();
    for p in logs {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for line in text.lines().rev().filter(|l| POSITION_EVENTS.iter().any(|e| l.contains(e))) {
            let Ok(l) = astromech_journal::parse_line(line.trim()) else { continue };
            match l.event {
                JournalEvent::Location { star_system, system_address, star_pos: Some(pos), .. }
                | JournalEvent::FsdJump { star_system, system_address, star_pos: Some(pos), .. } => {
                    return Some(Here { system: star_system, id64: system_address, pos });
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use astromech_data::{Cache, Client, Limiter, Request, Response, Transport, TransportError};

    /// Answers EDSM sphere and Spansh systems search with recorded bodies; logs request URLs.
    struct Recorded {
        sphere: String,
        log: Mutex<Vec<String>>,
    }

    impl Transport for Recorded {
        fn send(&self, url: &str, _req: &Request) -> Result<Response, TransportError> {
            self.log.lock().unwrap().push(url.to_string());
            let body = if url.ends_with("/api-v1/sphere-systems") {
                self.sphere.clone()
            } else if url.ends_with("/api/systems/search") {
                include_str!("../tests/fixtures/spansh-famine.json").to_string()
            } else if url.ends_with("/api/stations/search") {
                let all: serde_json::Value =
                    serde_json::from_str(include_str!("../tests/fixtures/spansh-services.json")).unwrap();
                all["refuel"].to_string()
            } else if url.ends_with("/api/systems/field_values/system_names") {
                SHIN.to_string()
            } else {
                panic!("unexpected request {url}")
            };
            Ok(Response { status: 200, body })
        }
    }

    #[test]
    fn nearby_fetches_through_the_data_client_then_reuses_the_cache() {
        // EDSM knows a dense line of stars from Sol towards the fixture systems.
        let sphere: Vec<serde_json::Value> = (0..40)
            .map(|i| {
                serde_json::json!({
                    "name": format!("S{i}"), "id64": 1000 + i,
                    "coords": { "x": i as f64 * 0.5, "y": -(i as f64) * 0.1, "z": i as f64 * 0.3 },
                })
            })
            .collect();
        let rec = Arc::new(Recorded { sphere: serde_json::Value::from(sphere).to_string(), log: Mutex::new(vec![]) });
        let client = Client::with_transport(Cache::in_memory().unwrap(), rec.clone()).limiter(Limiter::none());
        let g = Galaxy::with_remote(Store::in_memory().unwrap(), client);
        let here = Here { system: "Sol".into(), id64: Some(10477373803), pos: [0.0; 3] };

        let n = g.nearby("famine", &here, 5.0, 2, &Filters::default()).unwrap();
        {
            let log = rec.log.lock().unwrap();
            assert_eq!(log.len(), 2, "one EDSM fill-in and one Spansh search: {log:?}");
            assert!(log[0].starts_with("https://www.edsm.net/api-v1/sphere-systems"));
            assert_eq!(log[1], "https://spansh.co.uk/api/systems/search");
        }
        assert_eq!(n.hits.len(), 2);
        assert_eq!(n.hits[0].name, "WISE 0855-0714");
        assert!(n.hits[0].hop.exact, "jump counts use the filled-in map");
        assert!(n.hits[0].hop.jumps >= 2);
        assert!(g.position("S10").unwrap().is_some(), "EDSM systems are stored");

        let again = g.nearby("famine", &here, 5.0, 2, &Filters::default()).unwrap();
        assert_eq!(rec.log.lock().unwrap().len(), 2, "covered and cached: no requests");
        assert_eq!(again.hits, n.hits);
        assert_eq!(again.fetched, n.fetched);
    }

    #[test]
    fn stations_lists_the_nearest_with_jumps_from_the_typed_origin() {
        let rec = Arc::new(Recorded { sphere: "[]".into(), log: Mutex::new(vec![]) });
        let client = Client::with_transport(Cache::in_memory().unwrap(), rec.clone()).limiter(Limiter::none());
        let s = Store::in_memory().unwrap();
        s.add_coverage(Sphere { center: [0.0; 3], radius: 500.0 }, "test", "t").unwrap();
        let g = Galaxy::with_remote(s, client);
        let here = Here { system: "Sol".into(), id64: Some(10477373803), pos: [0.0; 3] };
        let want = stations::Want::Service { service: stations::Service::Refuel };
        let f = Filters { exclude_odyssey: true, ..Filters::default() };
        let n = g.stations(&want, &here, 20.0, 8, &f).unwrap();
        assert_eq!(n.want, "refuel");
        assert_eq!(n.filters, f);
        let names: Vec<&str> = n.hits.iter().map(|h| h.station.name.as_str()).collect();
        assert_eq!(names, ["Ehrlich City", "Furukawa Enterprise"], "settlements filtered out");
        assert_eq!((n.hits[0].jumps, n.hits[0].station.pad), (0, Some(Pad::L)), "Ehrlich City is in Sol");
        let log = rec.log.lock().unwrap();
        assert_eq!(*log, ["https://spansh.co.uk/api/stations/search"], "covered: no EDSM fill-in");
        let json = serde_json::to_value(&n).unwrap();
        assert_eq!(json["hits"][0]["system"], "Sol");
        assert_eq!(json["hits"][0]["exact"], true);
    }

    /// Spansh's autocomplete for "Shin" (recorded 16 Oct 2026, trimmed).
    const SHIN: &str = r#"{"min_max":[{"id64":1733321298650,"name":"Shingon","x":121.6875,"y":-3.09375,"z":47.1875},
        {"id64":3932277478106,"name":"Shinrarta Dezhra","x":55.71875,"y":17.59375,"z":27.15625}],
        "values":["Shingon","Shinrarta Dezhra"]}"#;

    #[test]
    fn typed_origin_is_found_in_the_map_then_on_spansh() {
        let rec = Arc::new(Recorded { sphere: "[]".into(), log: Mutex::new(vec![]) });
        let client = Client::with_transport(Cache::in_memory().unwrap(), rec.clone()).limiter(Limiter::none());
        let mut s = Store::in_memory().unwrap();
        s.insert_systems([(10477373803, "Sol", [0.0, 0.0, 0.0])]).unwrap();
        let g = Galaxy::with_remote(s, client);

        let sol = g.locate(" sol ").unwrap();
        assert_eq!((sol.system.as_str(), sol.id64, sol.pos), ("Sol", Some(10477373803), [0.0; 3]));
        assert!(rec.log.lock().unwrap().is_empty(), "a known system needs no request");

        let names = g.system_names("Shin").unwrap();
        assert_eq!(names, ["Shingon", "Shinrarta Dezhra"]);
        let sd = g.locate("shinrarta dezhra").unwrap();
        assert_eq!(sd.system, "Shinrarta Dezhra", "Spansh's spelling");
        assert_eq!(sd.id64, Some(3932277478106));
        assert_eq!(sd.pos, [55.71875, 17.59375, 27.15625]);

        let err = g.locate("Shin").unwrap_err().to_string();
        assert_eq!(err, "Spansh doesn't know a system called Shin.", "a prefix is not a system");
    }

    #[test]
    fn nearby_ranks_cached_hits_by_jumps() {
        let mut s = Store::in_memory().unwrap();
        // a dense line of stars from Sol towards the fixture systems
        let line: Vec<(i64, String, [f64; 3])> =
            (0..40).map(|i| (1000 + i, format!("S{i}"), [i as f64 * 0.5, -(i as f64) * 0.1, i as f64 * 0.3])).collect();
        s.insert_systems(line.iter().map(|(a, b, c)| (*a, b.as_str(), *c))).unwrap();
        s.add_coverage(Sphere { center: [0.0; 3], radius: 500.0 }, "test", "t").unwrap();
        let body = include_str!("../tests/fixtures/spansh-famine.json");
        let here = Here { system: "Sol".into(), id64: Some(10477373803), pos: [0.0; 3] };
        let key = "spansh:famine:0:0:0";
        s.cache_put(key, now_s(), body).unwrap();
        let g = Galaxy::with_store(s);
        // covered → no EDSM call; cached → no Spansh call
        let n = g.nearby("famine", &here, 5.0, 2, &Filters::default()).unwrap();
        assert_eq!(n.hits.len(), 2);
        assert_eq!(n.hits[0].name, "WISE 0855-0714");
        assert!(n.hits[0].hop.jumps >= 2);
        assert!(n.hits.iter().all(|h| h
            .factions
            .iter()
            .all(|f| f.state.as_deref() == Some("Famine") || f.active_states.iter().any(|s| s == "Famine"))));
        assert!(n.warning.is_none());
        // Spansh hits are added to the map
        assert!(g.position("Tau Ceti").unwrap().is_some());
        // the lookup's filters apply to the cached answer: every fixture hit is older than a day
        let fresh = Filters { max_age_days: Some(1), ..Filters::default() };
        let none = g.nearby("famine", &here, 5.0, 2, &fresh).unwrap();
        assert!(none.hits.is_empty() && none.warning.is_none(), "{none:?}");
    }

    #[test]
    fn hge_lookups_rank_by_population_and_distance() {
        let sys = |id: i64, name: &str, x: f64, pop: i64| {
            serde_json::json!({
                "id64": id, "name": name, "x": x, "y": 0.0, "z": 0.0, "population": pop,
                "updated_at": "2026-10-16T10:00:00Z",
                "minor_faction_presences": [{ "name": format!("{name} Party"), "allegiance": "Federation",
                    "state": "None", "active_states": [], "influence": 0.5 }],
            })
        };
        // nearest first, as Spansh sorts them
        let body = serde_json::json!({ "count": 3, "from": 0, "results": [
            sys(1, "Small Fed", 5.0, 200_000_000),
            sys(2, "Big Fed", 30.0, 2_000_000_000),
            sys(3, "Far Giant", 400.0, 9_000_000_000),
        ]})
        .to_string();
        let s = Store::in_memory().unwrap();
        s.add_coverage(Sphere { center: [0.0; 3], radius: 1000.0 }, "test", "t").unwrap();
        s.cache_put("spansh:fed_composites:0:0:0", now_s(), &body).unwrap();
        s.cache_put("spansh:fed_composites:pop:0:0:0", now_s(), EMPTY_PAGE).unwrap();
        s.cache_put("spansh:famine:0:0:0", now_s(), &body.replace("\"None\"", "\"Famine\"")).unwrap();
        let g = Galaxy::with_store(s);
        let here = Here { system: "Origin".into(), id64: None, pos: [0.0; 3] };

        let n = g.nearby("fed_composites", &here, 20.0, 3, &Filters::default()).unwrap();
        let ranked: Vec<(&str, Option<u32>)> = n.hits.iter().map(|h| (h.name.as_str(), h.score)).collect();
        assert_eq!(ranked, [("Big Fed", Some(72)), ("Far Giant", Some(16)), ("Small Fed", Some(5))]);

        // a faction-state lookup that isn't about HGEs keeps nearest first and has no score
        let f = g.nearby("famine", &here, 20.0, 3, &Filters::default()).unwrap();
        let ranked: Vec<(&str, Option<u32>)> = f.hits.iter().map(|h| (h.name.as_str(), h.score)).collect();
        assert_eq!(ranked, [("Small Fed", None), ("Big Fed", None), ("Far Giant", None)]);
    }

    const EMPTY_PAGE: &str = r#"{"count": 0, "from": 0, "results": []}"#;

    /// Answers the plain and the population-filtered Spansh systems search separately.
    struct TwoPages {
        nearest: String,
        big: String,
        log: Mutex<Vec<serde_json::Value>>,
    }

    impl Transport for TwoPages {
        fn send(&self, url: &str, req: &Request) -> Result<Response, TransportError> {
            let astromech_data::Body::Json(b) = &req.body else {
                return Ok(Response { status: 200, body: "[]".into() }); // EDSM fill-in
            };
            assert!(url.ends_with("/api/systems/search"), "{url}");
            self.log.lock().unwrap().push(b.clone());
            let body = if b["filters"].get("population").is_some() { &self.big } else { &self.nearest };
            Ok(Response { status: 200, body: body.clone() })
        }
    }

    #[test]
    fn hge_lookups_also_ask_for_the_nearest_billion_people_systems() {
        let sys = |id: i64, name: &str, x: f64, pop: i64| {
            serde_json::json!({
                "id64": id, "name": name, "x": x, "y": 0.0, "z": 0.0, "population": pop,
                "updated_at": "2026-10-16T10:00:00Z",
                "minor_faction_presences": [{ "name": format!("{name} Party"), "allegiance": "Federation",
                    "state": "None", "active_states": [], "influence": 0.5 }],
            })
        };
        let page =
            |r: &[serde_json::Value]| serde_json::json!({ "count": r.len(), "from": 0, "results": r }).to_string();
        let galaxy = |nearest: &[serde_json::Value], big: &[serde_json::Value]| {
            let rec = Arc::new(TwoPages { nearest: page(nearest), big: page(big), log: Mutex::new(vec![]) });
            let client = Client::with_transport(Cache::in_memory().unwrap(), rec.clone()).limiter(Limiter::none());
            let s = Store::in_memory().unwrap();
            s.add_coverage(Sphere { center: [0.0; 3], radius: 1000.0 }, "test", "t").unwrap();
            (Galaxy::with_remote(s, client), rec)
        };
        let here = Here { system: "Origin".into(), id64: None, pos: [0.0; 3] };
        // the nearest 50 matching systems are all small, as in the bubble; the population page
        // has a big one 60 ly out
        let small: Vec<_> =
            (0..50).map(|i| sys(100 + i, &format!("Small {i}"), 2.0 + i as f64 * 0.3, 5_000_000)).collect();
        let (g, rec) = galaxy(&small, &[sys(8, "Big Fed", 60.0, 3_000_000_000)]);
        let n = g.nearby("fed_composites", &here, 20.0, 3, &Filters::default()).unwrap();
        let names: Vec<&str> = n.hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["Big Fed", "Small 0", "Small 1"], "the farther billion-people system wins");
        {
            let log = rec.log.lock().unwrap();
            assert_eq!(log.len(), 2, "the nearest page and the population page");
            assert!(log[0]["filters"].get("population").is_none());
            assert_eq!(log[1]["filters"]["population"]["value"][0], "1000000000");
            assert_eq!(log[1]["filters"]["population"]["comparison"], "<=>");
        }
        // famine needs no population page
        g.nearby("famine", &here, 20.0, 3, &Filters::default()).unwrap();
        assert_eq!(rec.log.lock().unwrap().len(), 3);
        assert!(rec.log.lock().unwrap()[2]["filters"].get("population").is_none());

        // a big system in both pages is listed once
        let mut near = small.clone();
        near[49] = sys(7, "Near Billion", 16.7, 1_200_000_000);
        let (g, _) =
            galaxy(&near, &[sys(7, "Near Billion", 16.7, 1_200_000_000), sys(8, "Big Fed", 60.0, 3_000_000_000)]);
        let n = g.nearby("fed_composites", &here, 20.0, 5, &Filters::default()).unwrap();
        let names: Vec<&str> = n.hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names[..2], ["Near Billion", "Big Fed"]);
        assert_eq!(names.iter().filter(|n| **n == "Near Billion").count(), 1, "deduplicated");
    }

    #[test]
    fn hge_score_prefers_a_billion_people() {
        assert!(hge_score(2_000_000_000, 30.0) > hge_score(200_000_000, 5.0));
        assert!(hge_score(1_000_000_000, 100.0) > hge_score(999_000_000, 0.0));
        assert!(hge_score(5_000_000_000, 10.0) > hge_score(2_000_000_000, 10.0));
        assert!(hge_score(2_000_000_000, 10.0) > hge_score(2_000_000_000, 20.0));
        assert_eq!(hge_score(0, 0.0), 0.0);
    }

    #[test]
    fn arrival_faction_states_replace_older_spansh_ones() {
        let mut s = Store::in_memory().unwrap();
        s.insert_systems([(1, "S", [0.0, 0.0, 0.0])]).unwrap();
        s.add_coverage(Sphere { center: [0.0; 3], radius: 500.0 }, "test", "t").unwrap();
        s.cache_put("spansh:famine:0:0:0", now_s(), include_str!("../tests/fixtures/spansh-famine.json")).unwrap();
        let g = Galaxy::with_store(s);
        let here = Here { system: "Sol".into(), id64: Some(10477373803), pos: [0.0; 3] };
        let before = g.nearby("famine", &here, 20.0, 5, &Filters::default()).unwrap();
        let ross = before.hits.iter().find(|h| h.name == "Ross 154").unwrap();
        assert_eq!((ross.updated_at.as_deref(), ross.seen_on_visit), (Some("2026-10-14T16:32:04Z"), false));
        assert!(before.hits.iter().any(|h| h.name == "Tau Ceti"));

        // replay the commander's arrivals on 16 Oct (Spansh's copy is from 14 Oct)
        let lines: Vec<_> = include_str!("../tests/fixtures/journal-arrivals.log")
            .lines()
            .map(|l| astromech_journal::parse_line(l).unwrap())
            .collect();
        g.record_visits(&visits_from_lines(&lines)).unwrap();
        let after = g.nearby("famine", &here, 20.0, 5, &Filters::default()).unwrap();
        let ross = after.hits.iter().find(|h| h.name == "Ross 154").unwrap();
        assert_eq!(ross.updated_at.as_deref(), Some("2026-10-16T12:00:00Z"), "age from the journal");
        assert!(ross.seen_on_visit);
        assert_eq!(ross.factions.len(), 1);
        assert_eq!(ross.factions[0].active_states, ["Famine", "Civil Unrest"], "states from the journal");
        // Tau Ceti's famine ended on arrival: no longer suggested
        assert!(!after.hits.iter().any(|h| h.name == "Tau Ceti"), "{:?}", after.hits);
        // an older visit doesn't undo the newer one
        let mut old = visits_from_lines(&lines);
        old.iter_mut().for_each(|v| v.seen = "2026-10-01T00:00:00Z".into());
        g.record_visits(&old).unwrap();
        let again = g.nearby("famine", &here, 20.0, 5, &Filters::default()).unwrap();
        assert_eq!(again.hits, after.hits);
        // and the boom shows up for the proto lookup
        let s2 = g.store();
        s2.cache_put("spansh:proto:0:0:0", now_s(), include_str!("../tests/fixtures/spansh-famine.json")).unwrap();
        s2.cache_put("spansh:proto:pop:0:0:0", now_s(), EMPTY_PAGE).unwrap();
        drop(s2);
        let proto = g.nearby("proto", &here, 20.0, 5, &Filters::default()).unwrap();
        let tau = proto.hits.iter().find(|h| h.name == "Tau Ceti").unwrap();
        assert!(tau.seen_on_visit && tau.factions[0].name == "Independents of Tau Ceti");
    }

    #[test]
    fn farming_notes_name_known_lookups() {
        for spec in astromech_materials::catalog::catalog() {
            for n in astromech_materials::hge::sources_for(spec) {
                if let Some(q) = n.query.as_deref() {
                    assert!(query(q).is_some(), "{} names unknown lookup {q}", spec.id);
                }
            }
        }
        assert!(
            astromech_materials::hge::sources_for(astromech_materials::lookup("dataminedwake").unwrap())[0]
                .query
                .as_deref()
                == Some("famine")
        );
    }

    #[test]
    fn here_from_journals() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("Journal.2026-10-01T000000.01.log"),
            "{\"timestamp\":\"t\",\"event\":\"FSDJump\",\"StarSystem\":\"Sol\",\"SystemAddress\":10477373803,\"StarPos\":[0,0,0]}\n{\"timestamp\":\"t\",\"event\":\"FSDJump\",\"StarSystem\":\"Deciat\",\"SystemAddress\":6681123623626,\"StarPos\":[122.625,-0.8125,-47.28125]}\n{\"timestamp\":\"t\",\"event\":\"Docked\",\"StationName\":\"X\"}\n",
        )
        .unwrap();
        let h = here_from_journal_dir(d.path()).unwrap();
        assert_eq!(h.system, "Deciat");
        assert_eq!(h.pos[0], 122.625);
    }
}
