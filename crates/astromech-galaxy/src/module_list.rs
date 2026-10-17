//! Where to buy a list of modules (Inara's "all in one station"): stations near the origin that
//! sell any of them, ranked by how many of the list they sell, nearest first among equals, with
//! what each sells and at what price.
//!
//! One Spansh stations search per module (its nearest sellers) and, for two or more modules, one
//! asking for all of them at once. Spansh ANDs the entries of its `modules` filter (checked
//! 16 Oct 2026: 82, 24 and 152 stations within 20 ly of Sol sell a 5A FSD, a 4A distributor and
//! a 0A booster, 13 sell all three), so that search finds all-in-one stations beyond each
//! module's nearest sellers. Every station Spansh returns lists everything it sells, so each is
//! checked against the whole list whichever search found it, and the search kit's filters are
//! re-applied as in [`crate::stations::find`].

use crate::filters::{Filters, Pad};
use crate::outfitting::{spansh_names, ModuleWant};
use crate::stations::{keeps, pad_of, Want};
use astromech_data::spansh::{Reference, Search, Station};
use astromech_data::Client;
use serde::Serialize;
use std::collections::BTreeMap;

/// The id the search kit remembers this lookup's filters under.
pub const LOOKUP: &str = "stations.modules";

/// One listed module a station sells.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sold {
    /// The listed module ("5A Frame Shift Drive").
    pub module: String,
    /// Its cheapest fitting price, in credits.
    pub price: Option<i64>,
    /// The sizes and mounts it is sold in there.
    pub detail: Option<String>,
}

/// A station selling some of the list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outfitter {
    pub name: String,
    pub station_type: Option<String>,
    pub market_id: Option<i64>,
    pub system: String,
    pub system_id64: i64,
    pub pos: [f64; 3],
    /// Light years from where the search measured.
    pub ly: f64,
    pub arrival_ls: Option<f64>,
    pub pad: Option<Pad>,
    /// When its outfitting was last reported (RFC 3339).
    pub updated_at: Option<String>,
    /// What it sells, in list order.
    pub sold: Vec<Sold>,
    /// The listed modules it doesn't sell, in list order.
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleList {
    /// Most of the list first, then nearest.
    pub stations: Vec<Outfitter>,
    /// Listed modules no station found sells.
    pub unavailable: Vec<String>,
    /// When Spansh answered (the oldest answer used; unix seconds). 0 with nothing listed.
    pub fetched_at: i64,
    pub warning: Option<String>,
}

/// Stations (up to `limit`) selling any of `wants` near `reference` that pass `filters`, most of
/// the list first, then nearer. A module listed twice counts once.
pub fn find(
    client: &Client,
    wants: &[ModuleWant],
    reference: Reference,
    filters: &Filters,
    limit: usize,
    now_s: i64,
) -> astromech_data::Result<ModuleList> {
    let mut each: Vec<Want> = vec![];
    for w in wants {
        let want = Want::Module(w.clone());
        if !each.contains(&want) {
            each.push(want);
        }
    }
    let Some(first) = each.first() else {
        return Ok(ModuleList { stations: vec![], unavailable: vec![], fetched_at: 0, warning: None });
    };
    let mut searches: Vec<Search> = each.iter().map(|w| w.search(reference.clone(), filters, now_s)).collect();
    if each.len() > 1 {
        let all = each[1..].iter().fold(searches[0].clone(), |q, w| match w {
            Want::Module(m) => q.module(&spansh_names(&m.name), m.class, m.rating.as_deref()),
            _ => q,
        });
        searches.push(all);
    }

    let mut fetched_at = i64::MAX;
    let mut warnings: Vec<String> = vec![];
    // Keyed by system and name: two stations in different systems can share a name.
    let mut found: BTreeMap<(i64, String), Station> = BTreeMap::new();
    for (i, q) in searches.iter().enumerate() {
        let page = client.spansh_stations(q)?;
        fetched_at = fetched_at.min(page.fetched_at);
        let results = page.value.results;
        if let Some(w) = each.get(i) {
            if !results.is_empty() && !results.iter().any(|s| w.offer(s).is_some()) {
                warnings.push(w.none_offered(&results));
            }
        }
        for s in results {
            found.entry((s.system_id64, s.name.clone())).or_insert(s);
        }
    }

    let mut stations: Vec<Outfitter> = found
        .into_values()
        .filter(|s| keeps(first, s, filters, now_s))
        .filter_map(|s| {
            let mut sold = vec![];
            let mut missing = vec![];
            for w in &each {
                match w.offer(&s) {
                    Some(o) => sold.push(Sold { module: w.label(), price: o.price, detail: o.detail }),
                    None => missing.push(w.label()),
                }
            }
            (!sold.is_empty()).then(|| Outfitter {
                pad: pad_of(&s),
                updated_at: first.updated_at(&s).map(str::to_string),
                pos: s.pos(),
                name: s.name,
                station_type: s.station_type,
                market_id: s.market_id,
                system: s.system_name,
                system_id64: s.system_id64,
                ly: s.distance,
                arrival_ls: s.distance_to_arrival,
                sold,
                missing,
            })
        })
        .collect();
    stations.sort_by(|a, b| {
        b.sold
            .len()
            .cmp(&a.sold.len())
            .then(a.ly.total_cmp(&b.ly))
            .then_with(|| (&a.system, &a.name).cmp(&(&b.system, &b.name)))
    });
    let unavailable = each
        .iter()
        .map(Want::label)
        .filter(|l| !stations.iter().any(|s| s.sold.iter().any(|x| &x.module == l)))
        .collect();
    stations.truncate(limit.max(1));
    Ok(ModuleList { stations, unavailable, fetched_at, warning: (!warnings.is_empty()).then(|| warnings.join(" ")) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use astromech_data::{Cache, Limiter, Request, Response, Transport, TransportError};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    /// Spansh's answers near Sol for a 5A Frame Shift Drive, a 4A Power Distributor and a 0A
    /// Shield Booster, one at a time and all three at once, with and without fleet carriers
    /// (recorded 16 Oct 2026, trimmed to the eight nearest stations and the three modules).
    const PAGES: &str = include_str!("../tests/fixtures/spansh-module-list.json");

    /// Answers each stations search with the recorded page for the modules it asks for, and
    /// keeps the request bodies.
    struct Recorded {
        bodies: Mutex<Vec<Value>>,
    }

    impl Transport for Recorded {
        fn send(&self, url: &str, req: &Request) -> Result<Response, TransportError> {
            assert_eq!(url, "https://spansh.co.uk/api/stations/search");
            let body = match &req.body {
                astromech_data::Body::Json(v) => v.clone(),
                other => panic!("expected JSON, got {other:?}"),
            };
            self.bodies.lock().unwrap().push(body.clone());
            let modules = body["filters"]["modules"].as_array().unwrap();
            let key = match modules.as_slice() {
                [m] => match m["name"][0].as_str().unwrap() {
                    "Frame Shift Drive" => "fsd_5a",
                    "Power Distributor" => "distributor_4a",
                    "Shield Booster" => "booster_0a",
                    other => panic!("no recorded page for {other}"),
                },
                _ => "all",
            };
            let key = if body["filters"].get("type").is_some() { format!("{key}_no_carriers") } else { key.into() };
            let all: Value = serde_json::from_str(PAGES).unwrap();
            Ok(Response { status: 200, body: all[&key].to_string() })
        }
    }

    fn client() -> (Client, Arc<Recorded>) {
        let rec = Arc::new(Recorded { bodies: Mutex::new(vec![]) });
        (Client::with_transport(Cache::in_memory().unwrap(), rec.clone()).limiter(Limiter::none()), rec)
    }

    const NOW: i64 = 1_792_152_000; // 2026-10-16T12:00:00Z (about when the fixtures were recorded)

    fn sol() -> Reference {
        Reference::System("Sol".into())
    }

    /// A plan needing three modules: a 5A FSD, a 4A distributor and a 0A booster.
    fn three() -> Vec<ModuleWant> {
        vec![
            ModuleWant::from_words("Frame Shift Drive", "5a"),
            ModuleWant::from_words("Power Distributor", "4a"),
            // Utility mounts are class 0, which spoken words don't give.
            ModuleWant { name: "Shield Booster".into(), class: Some(0), rating: Some("A".into()), mount: None },
        ]
    }

    fn names(l: &ModuleList) -> Vec<(&str, usize)> {
        l.stations.iter().map(|s| (s.name.as_str(), s.sold.len())).collect()
    }

    #[test]
    fn a_station_selling_all_three_ranks_above_a_closer_one_selling_two() {
        let (c, _) = client();
        let l = find(&c, &three(), sol(), &Filters::default(), 12, NOW).unwrap();
        // Abraham Lincoln (Sol, 0 ly) sells the FSD and the booster; G6G-8KJ (5.95 ly) all three.
        let lincoln = l.stations.iter().position(|s| s.name == "Abraham Lincoln").unwrap();
        let g6g = l.stations.iter().position(|s| s.name == "G6G-8KJ").unwrap();
        assert!(g6g < lincoln, "{:?}", names(&l));
        assert_eq!(l.stations[lincoln].sold.len(), 2);
        assert_eq!(l.stations[lincoln].missing, ["4A Power Distributor"]);
        assert!(l.stations[lincoln].ly < l.stations[g6g].ly);
        assert_eq!(l.stations[g6g].sold.len(), 3);
        assert!(l.stations[g6g].missing.is_empty());
        let sold: Vec<&str> = l.stations[g6g].sold.iter().map(|s| s.module.as_str()).collect();
        assert_eq!(sold, ["5A Frame Shift Drive", "4A Power Distributor", "0A Shield Booster"], "list order");
        assert!(l.stations[g6g].sold.iter().all(|s| s.price.is_some()));
        // Most of the list first, then nearest.
        assert!(l.stations.windows(2).all(|w| {
            w[0].sold.len() > w[1].sold.len() || (w[0].sold.len() == w[1].sold.len() && w[0].ly <= w[1].ly)
        }));
        assert!(l.unavailable.is_empty() && l.warning.is_none());
    }

    #[test]
    fn one_search_per_module_and_one_for_all_of_them() {
        let (c, rec) = client();
        find(&c, &three(), sol(), &Filters::default(), 12, NOW).unwrap();
        let bodies = rec.bodies.lock().unwrap();
        assert_eq!(bodies.len(), 4);
        let mods = |b: &Value| b["filters"]["modules"].as_array().unwrap().len();
        assert_eq!(bodies.iter().map(mods).collect::<Vec<_>>(), [1, 1, 1, 3]);
        assert_eq!(
            bodies[3]["filters"]["modules"][1],
            serde_json::json!({ "name": ["Power Distributor"], "class": ["4"], "rating": ["A"] })
        );
        drop(bodies);
        // One module: no all-of search; a module listed twice counts once.
        let (c, rec) = client();
        let fsd = ModuleWant::from_words("Frame Shift Drive", "5a");
        let l = find(&c, &[fsd.clone(), fsd], sol(), &Filters::default(), 12, NOW).unwrap();
        assert_eq!(rec.bodies.lock().unwrap().len(), 1);
        assert!(l.stations.iter().all(|s| s.sold.len() == 1 && s.missing.is_empty()));
    }

    #[test]
    fn the_all_of_search_finds_all_in_one_stations_past_each_modules_nearest() {
        // Without carriers, none of the eight nearest sellers of each module sells all three;
        // the all-of search finds Bugrov Hub at 32 ly.
        let (c, rec) = client();
        let f = Filters { exclude_carriers: true, ..Filters::default() };
        let l = find(&c, &three(), sol(), &f, 30, NOW).unwrap();
        assert_eq!(names(&l)[0], ("Bugrov Hub", 3));
        assert!(l.stations.iter().all(|s| s.station_type.as_deref() != Some("Drake-Class Carrier")));
        let carriers = rec.bodies.lock().unwrap().iter().filter(|b| b["filters"]["type"].is_object()).count();
        assert_eq!(carriers, 4, "every search sends the station types");
        // Without that search the best station would sell two.
        let all: Value = serde_json::from_str(PAGES).unwrap();
        let in_all: Vec<&str> =
            all["all_no_carriers"]["results"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
        let first_two = l.stations.iter().find(|s| s.sold.len() == 2).unwrap();
        assert!(!in_all.contains(&first_two.name.as_str()));
    }

    #[test]
    fn stations_sharing_a_name_in_different_systems_stay_apart() {
        let (c, _) = client();
        let l = find(&c, &three(), sol(), &Filters::default(), 40, NOW).unwrap();
        let strongholds: Vec<&Outfitter> = l.stations.iter().filter(|s| s.name == "Stronghold Carrier").collect();
        assert_eq!(strongholds.len(), 2);
        assert_ne!(strongholds[0].system, strongholds[1].system);
    }

    #[test]
    fn filters_are_rechecked_and_the_limit_applies() {
        let (c, _) = client();
        let large = Filters { pad: Some(Pad::L), ..Filters::default() };
        let l = find(&c, &three(), sol(), &large, 40, NOW).unwrap();
        assert!(!l.stations.is_empty());
        assert!(l.stations.iter().all(|s| s.pad == Some(Pad::L)), "outposts (medium pads) go");
        let l = find(&c, &three(), sol(), &Filters::default(), 3, NOW).unwrap();
        assert_eq!(l.stations.len(), 3);
        assert!(l.stations.iter().all(|s| s.sold.len() == 3));
    }

    #[test]
    fn a_module_no_station_sells_is_named() {
        // A gimballed booster doesn't exist; the recorded pages answer the booster search with
        // fixed ones, which Spansh can't tell apart by mount.
        let (c, _) = client();
        let mut wants = three();
        wants[2].mount = Some(crate::outfitting::Mount::Gimballed);
        let l = find(&c, &wants, sol(), &Filters::default(), 12, NOW).unwrap();
        assert_eq!(l.unavailable, ["0A Shield Booster (gimballed)"]);
        assert!(l.stations.iter().all(|s| s.sold.len() <= 2));
        assert!(l.warning.is_some());
    }

    #[test]
    fn nothing_listed_asks_nothing() {
        let (c, rec) = client();
        let l = find(&c, &[], sol(), &Filters::default(), 12, NOW).unwrap();
        assert!(l.stations.is_empty() && l.fetched_at == 0);
        assert!(rec.bodies.lock().unwrap().is_empty());
    }
}
