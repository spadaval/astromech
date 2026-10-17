//! Station lookups: the nearest stations offering a service or selling a module or ship, over Spansh's stations search
//! (through `astromech-data`). The search kit's filters go to Spansh where it has a verified
//! filter for them, so a strict filter still finds stations farther out; every hit is then
//! re-checked here, because Spansh silently ignores a filter it doesn't recognise.

use crate::filters::{Facts, Filters, Pad};
use crate::outfitting::{size_label, ModuleWant};
use astromech_data::spansh::{self, Reference, Search, Station, CARRIER_TYPES, SETTLEMENT_TYPES, STATION_TYPES};
use astromech_data::Client;
use serde::{Deserialize, Serialize};

/// Stations asked for per search: enough that client-side filters rarely empty the page.
pub const SEARCH_SIZE: usize = 40;

/// A station service the lookups offer. Traders and brokers come by type, because they trade
/// different things.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Service {
    InterstellarFactors,
    MaterialTrader,
    RawTrader,
    ManufacturedTrader,
    EncodedTrader,
    TechnologyBroker,
    HumanBroker,
    GuardianBroker,
    UniversalCartographics,
    VistaGenomics,
    PioneerSupplies,
    Bartender,
    FrontlineSolutions,
    Apex,
    Refuel,
    Repair,
    Rearm,
    BlackMarket,
    SearchAndRescue,
}

/// How Spansh knows a service: by name in a station's `services`, or as the station's material
/// trader or technology broker type.
enum Spansh {
    Service(&'static str),
    Trader(&'static str),
    Broker(&'static str),
}

impl Service {
    pub const ALL: [Service; 19] = [
        Service::InterstellarFactors,
        Service::MaterialTrader,
        Service::RawTrader,
        Service::ManufacturedTrader,
        Service::EncodedTrader,
        Service::TechnologyBroker,
        Service::HumanBroker,
        Service::GuardianBroker,
        Service::UniversalCartographics,
        Service::VistaGenomics,
        Service::PioneerSupplies,
        Service::Bartender,
        Service::FrontlineSolutions,
        Service::Apex,
        Service::Refuel,
        Service::Repair,
        Service::Rearm,
        Service::BlackMarket,
        Service::SearchAndRescue,
    ];

    /// The name to show and speak.
    pub fn label(self) -> &'static str {
        match self {
            Service::InterstellarFactors => "Interstellar Factors",
            Service::MaterialTrader => "material trader (any type)",
            Service::RawTrader => "raw material trader",
            Service::ManufacturedTrader => "manufactured material trader",
            Service::EncodedTrader => "encoded data trader",
            Service::TechnologyBroker => "technology broker (any type)",
            Service::HumanBroker => "human technology broker",
            Service::GuardianBroker => "Guardian technology broker",
            Service::UniversalCartographics => "Universal Cartographics",
            Service::VistaGenomics => "Vista Genomics",
            Service::PioneerSupplies => "Pioneer Supplies",
            Service::Bartender => "bartender",
            Service::FrontlineSolutions => "Frontline Solutions",
            Service::Apex => "Apex Interstellar",
            Service::Refuel => "refuel",
            Service::Repair => "repair",
            Service::Rearm => "rearm",
            Service::BlackMarket => "black market",
            Service::SearchAndRescue => "search and rescue",
        }
    }

    /// What the commander might say for it (the voice grammar's `{service}` slot).
    pub fn spoken(self) -> &'static [&'static str] {
        match self {
            Service::InterstellarFactors => &["interstellar factors", "factors", "interstellar factors contact"],
            Service::MaterialTrader => &["material trader", "materials trader", "mat trader"],
            Service::RawTrader => &["raw material trader", "raw materials trader", "raw trader"],
            Service::ManufacturedTrader => {
                &["manufactured material trader", "manufactured materials trader", "manufactured trader"]
            }
            Service::EncodedTrader => {
                &["encoded data trader", "encoded material trader", "encoded trader", "data trader"]
            }
            Service::TechnologyBroker => &["technology broker", "tech broker"],
            Service::HumanBroker => &["human technology broker", "human tech broker", "human broker"],
            Service::GuardianBroker => &["guardian technology broker", "guardian tech broker", "guardian broker"],
            Service::UniversalCartographics => &["universal cartographics", "cartographics", "cartographer"],
            Service::VistaGenomics => &["vista genomics", "vista"],
            Service::PioneerSupplies => &["pioneer supplies", "pioneer"],
            Service::Bartender => &["bartender", "bar"],
            Service::FrontlineSolutions => &["frontline solutions", "frontline"],
            Service::Apex => &["apex interstellar", "apex", "apex taxi"],
            Service::Refuel => &["refuel", "fuel station", "place to refuel"],
            Service::Repair => &["repair", "repairs", "place to repair"],
            Service::Rearm => &["rearm", "restock", "ammo"],
            Service::BlackMarket => &["black market"],
            Service::SearchAndRescue => &["search and rescue", "search rescue"],
        }
    }

    fn spansh(self) -> Spansh {
        match self {
            Service::InterstellarFactors => Spansh::Service("Interstellar Factors Contact"),
            Service::MaterialTrader => Spansh::Service("Material Trader"),
            Service::RawTrader => Spansh::Trader("Raw"),
            Service::ManufacturedTrader => Spansh::Trader("Manufactured"),
            Service::EncodedTrader => Spansh::Trader("Encoded"),
            Service::TechnologyBroker => Spansh::Service("Technology Broker"),
            Service::HumanBroker => Spansh::Broker("Human"),
            Service::GuardianBroker => Spansh::Broker("Guardian"),
            Service::UniversalCartographics => Spansh::Service("Universal Cartographics"),
            Service::VistaGenomics => Spansh::Service("Vista Genomics"),
            Service::PioneerSupplies => Spansh::Service("Pioneer Supplies"),
            Service::Bartender => Spansh::Service("Bartender"),
            Service::FrontlineSolutions => Spansh::Service("Frontline Solutions"),
            Service::Apex => Spansh::Service("Apex Interstellar"),
            Service::Refuel => Spansh::Service("Refuel"),
            Service::Repair => Spansh::Service("Repair"),
            Service::Rearm => Spansh::Service("Restock"),
            Service::BlackMarket => Spansh::Service("Black Market"),
            Service::SearchAndRescue => Spansh::Service("Search and Rescue"),
        }
    }

    pub fn id(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }

    pub fn from_id(id: &str) -> Option<Service> {
        serde_json::from_value(serde_json::Value::String(id.to_string())).ok()
    }
}

/// What a station lookup looks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Want {
    Service {
        service: Service,
    },
    /// `{"kind": "module", "name": "Multi-Cannon", "class": 3, "rating": "C", "mount": "Gimballed"}`
    Module(ModuleWant),
    /// A ship, by its name in game ("Krait MkII").
    Ship {
        ship: String,
    },
}

/// What a station offers of the thing asked for.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Offer {
    pub(crate) price: Option<i64>,
    pub(crate) detail: Option<String>,
}

impl Want {
    /// What is being looked for, to show and speak ("Interstellar Factors").
    pub fn label(&self) -> String {
        match self {
            Want::Service { service } => service.label().to_string(),
            Want::Module(m) => m.label(),
            Want::Ship { ship } => ship.clone(),
        }
    }

    /// The lookup id its filters are remembered under (`search.filters.<lookup>`).
    pub fn lookup(&self) -> &'static str {
        match self {
            Want::Service { .. } => "stations.service",
            Want::Module(_) => "stations.module",
            Want::Ship { .. } => "stations.ship",
        }
    }

    fn ask(&self, q: Search) -> Search {
        match self {
            Want::Service { service } => match service.spansh() {
                Spansh::Service(name) => q.service(name),
                Spansh::Trader(kind) => q.material_trader(kind),
                Spansh::Broker(kind) => q.technology_broker(kind),
            },
            Want::Module(m) => q.module(&crate::outfitting::spansh_names(&m.name), m.class, m.rating.as_deref()),
            Want::Ship { ship } => q.ship(ship),
        }
    }

    /// The station's offer, if it really has the thing (Spansh may have ignored the filter).
    pub(crate) fn offer(&self, s: &Station) -> Option<Offer> {
        let has = match self {
            Want::Service { service } => match service.spansh() {
                Spansh::Service(name) => s.has_service(name),
                Spansh::Trader(kind) => s.material_trader.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(kind)),
                Spansh::Broker(kind) => s.technology_broker.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(kind)),
            },
            Want::Module(want) => {
                let mut sold: Vec<&spansh::Module> = s.modules.iter().filter(|m| want.fits(m)).collect();
                sold.sort_by_key(|m| m.price.unwrap_or(i64::MAX));
                let first = sold.first()?;
                let mut sizes: Vec<String> = sold.iter().map(|m| size_label(m)).collect();
                sizes.dedup();
                return Some(Offer { price: first.price, detail: Some(sizes.join(", ")) });
            }
            Want::Ship { ship } => {
                // By name, or by journal symbol in case Spansh renames a ship.
                let symbol = crate::outfitting::ship(ship).map(|k| k.symbol.as_str());
                let sold = s.ships.iter().find(|x| {
                    x.name.eq_ignore_ascii_case(ship)
                        || x.symbol.as_deref().zip(symbol).is_some_and(|(have, want)| have.eq_ignore_ascii_case(want))
                })?;
                return Some(Offer { price: sold.price, detail: None });
            }
        };
        has.then(Offer::default)
    }

    /// Why a page of stations has none offering this: the mount (which Spansh can't filter)
    /// when they sell the module in other mounts, else Spansh ignored a filter.
    pub(crate) fn none_offered(&self, results: &[Station]) -> String {
        if let Want::Module(m @ ModuleWant { mount: Some(mount), .. }) = self {
            let any = Want::Module(ModuleWant { mount: None, ..m.clone() });
            if results.iter().any(|s| any.offer(s).is_some()) {
                return format!(
                    "None of the {} nearest stations selling {} sell it {}.",
                    results.len(),
                    any.label(),
                    mount.word()
                );
            }
        }
        format!("Spansh returned stations without {}; its search filter may have changed.", self.label())
    }

    /// The station field that says how old the answer is.
    fn age_field(&self) -> &'static str {
        match self {
            Want::Service { .. } => "updated_at",
            Want::Module(_) => "outfitting_updated_at",
            Want::Ship { .. } => "shipyard_updated_at",
        }
    }

    pub(crate) fn updated_at<'a>(&self, s: &'a Station) -> Option<&'a str> {
        match self {
            Want::Service { .. } => s.updated_at.as_deref(),
            Want::Module(_) => s.outfitting_updated_at.as_deref().or(s.updated_at.as_deref()),
            Want::Ship { .. } => s.shipyard_updated_at.as_deref().or(s.updated_at.as_deref()),
        }
    }

    /// `filters` with the pad raised to what the thing asked for needs: a ship can only be
    /// bought where it can land.
    pub fn filters(&self, filters: &Filters) -> Filters {
        let need = match self {
            Want::Ship { ship } => crate::outfitting::ship_pad(ship),
            _ => None,
        };
        Filters { pad: filters.pad.max(need), ..filters.clone() }
    }

    /// The Spansh request for this lookup near `reference` with `filters`. `now_s` dates the
    /// age filter (rounded down to the hour so repeated searches share a cache entry).
    pub fn search(&self, reference: Reference, filters: &Filters, now_s: i64) -> Search {
        let mut q = self.ask(Search::near(reference).size(SEARCH_SIZE));
        if filters.exclude_carriers || filters.exclude_odyssey {
            let types: Vec<&str> = STATION_TYPES
                .iter()
                .copied()
                .filter(|t| !(filters.exclude_carriers && CARRIER_TYPES.contains(t)))
                .filter(|t| !(filters.exclude_odyssey && SETTLEMENT_TYPES.contains(t)))
                .collect();
            q = q.types(&types);
        }
        if filters.pad == Some(Pad::L) {
            q = q.large_pad();
        }
        if let Some(ls) = filters.max_arrival_ls {
            q = q.max_arrival(ls as f64);
        }
        if let Some(ly) = filters.max_distance_ly {
            q = q.within(ly as f64);
        }
        if let Some(days) = filters.max_age_days {
            let since = (now_s - days as i64 * 86_400).div_euclid(3600) * 3600;
            q = q.updated_since(self.age_field(), &crate::iso(since));
        }
        q
    }
}

/// The largest pad at a station, from its pad counts (or Spansh's large-pad flag).
pub fn pad_of(s: &Station) -> Option<Pad> {
    let has = |n: Option<u32>| n.is_some_and(|n| n > 0);
    if has(s.large_pads) || s.has_large_pad == Some(true) {
        Some(Pad::L)
    } else if has(s.medium_pads) {
        Some(Pad::M)
    } else if has(s.small_pads) {
        Some(Pad::S)
    } else {
        None
    }
}

/// Whether `s` passes `filters`, its data age being the one `want` reads.
pub(crate) fn keeps(want: &Want, s: &Station, filters: &Filters, now_s: i64) -> bool {
    let t = s.station_type.as_deref().unwrap_or("");
    let facts = Facts {
        pad: pad_of(s),
        arrival_ls: s.distance_to_arrival,
        fleet_carrier: CARRIER_TYPES.contains(&t),
        odyssey: SETTLEMENT_TYPES.contains(&t),
        updated_at: want.updated_at(s),
        ly: Some(s.distance),
        volume: None,
    };
    filters.keeps(&facts, now_s)
}

/// One station offering what was asked for.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StationHit {
    pub name: String,
    pub station_type: Option<String>,
    pub market_id: Option<i64>,
    pub system: String,
    pub system_id64: i64,
    pub pos: [f64; 3],
    /// Light years from where the search measured.
    pub ly: f64,
    /// Light seconds from the arrival star.
    pub arrival_ls: Option<f64>,
    /// The largest pad.
    pub pad: Option<Pad>,
    /// When the answer was last reported (RFC 3339).
    pub updated_at: Option<String>,
    /// Price in credits, for what is sold.
    pub price: Option<i64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// Nearest first.
    pub hits: Vec<StationHit>,
    /// When Spansh answered (unix seconds; may be a cached answer).
    pub fetched_at: i64,
    pub warning: Option<String>,
}

/// The nearest stations (up to `limit`) offering `want` near `reference` that pass `filters`.
pub fn find(
    client: &Client,
    want: &Want,
    reference: Reference,
    filters: &Filters,
    limit: usize,
    now_s: i64,
) -> astromech_data::Result<Found> {
    let filters = &want.filters(filters);
    let page = client.spansh_stations(&want.search(reference, filters, now_s))?;
    let results = &page.value.results;
    let offered: Vec<(&Station, Offer)> = results.iter().filter_map(|s| Some((s, want.offer(s)?))).collect();
    let warning = (!results.is_empty() && offered.is_empty()).then(|| want.none_offered(results));
    let mut hits: Vec<StationHit> = offered
        .into_iter()
        .filter(|(s, _)| keeps(want, s, filters, now_s))
        .map(|(s, o)| StationHit {
            name: s.name.clone(),
            station_type: s.station_type.clone(),
            market_id: s.market_id,
            system: s.system_name.clone(),
            system_id64: s.system_id64,
            pos: s.pos(),
            ly: s.distance,
            arrival_ls: s.distance_to_arrival,
            pad: pad_of(s),
            updated_at: want.updated_at(s).map(str::to_string),
            price: o.price,
            detail: o.detail,
        })
        .collect();
    hits.sort_by(|a, b| a.ly.total_cmp(&b.ly));
    hits.truncate(limit.max(1));
    Ok(Found { hits, fetched_at: page.fetched_at, warning })
}

#[cfg(test)]
mod tests {
    use super::*;
    use astromech_data::{Cache, Limiter, Request, Response, Transport, TransportError};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    /// Spansh's answers for each service near Sol (recorded 16 Oct 2026, trimmed to five
    /// stations and the services that matter), keyed by [`Service::id`].
    const SERVICES: &str = include_str!("../tests/fixtures/spansh-services.json");

    /// Answers a stations search with the recorded page for the service it asks for (or
    /// `fixed`), and keeps the request bodies.
    struct Recorded {
        fixed: Option<String>,
        bodies: Mutex<Vec<Value>>,
    }

    impl Transport for Recorded {
        fn send(&self, url: &str, req: &Request) -> Result<Response, TransportError> {
            assert_eq!(url, "https://spansh.co.uk/api/stations/search");
            let body: Value = match &req.body {
                astromech_data::Body::Json(v) => v.clone(),
                other => panic!("expected JSON, got {other:?}"),
            };
            self.bodies.lock().unwrap().push(body.clone());
            if let Some(f) = &self.fixed {
                return Ok(Response { status: 200, body: f.clone() });
            }
            let f = &body["filters"];
            let all: Value = serde_json::from_str(SERVICES).unwrap();
            let key = Service::ALL
                .iter()
                .find(|s| {
                    let w = Want::Service { service: **s };
                    let probe = w.ask(Search::near(Reference::System("Sol".into()))).body();
                    probe["filters"].as_object().unwrap().iter().all(|(k, v)| f.get(k) == Some(v))
                })
                .map(|s| s.id())
                .expect("a recorded service");
            Ok(Response { status: 200, body: all[&key].to_string() })
        }
    }

    fn client(fixed: Option<String>) -> (Client, Arc<Recorded>) {
        let rec = Arc::new(Recorded { fixed, bodies: Mutex::new(vec![]) });
        (Client::with_transport(Cache::in_memory().unwrap(), rec.clone()).limiter(Limiter::none()), rec)
    }

    const NOW: i64 = 1_792_152_000; // 2026-10-16T12:00:00Z (about when the fixtures were recorded)

    fn sol() -> Reference {
        Reference::System("Sol".into())
    }

    #[test]
    fn every_service_finds_stations_that_offer_it_nearest_first() {
        let (c, _) = client(None);
        for s in Service::ALL {
            let want = Want::Service { service: s };
            let found = find(&c, &want, sol(), &Filters::default(), 5, NOW).unwrap();
            assert!(!found.hits.is_empty(), "{s:?}: no hits");
            assert!(found.warning.is_none(), "{s:?}: {:?}", found.warning);
            assert!(found.hits.windows(2).all(|w| w[0].ly <= w[1].ly), "{s:?} not nearest first");
        }
    }

    #[test]
    fn traders_and_brokers_are_only_the_asked_for_type() {
        let (c, rec) = client(None);
        let all: Value = serde_json::from_str(SERVICES).unwrap();
        for (s, field, kind) in [
            (Service::RawTrader, "material_trader", "Raw"),
            (Service::ManufacturedTrader, "material_trader", "Manufactured"),
            (Service::EncodedTrader, "material_trader", "Encoded"),
            (Service::HumanBroker, "technology_broker", "Human"),
            (Service::GuardianBroker, "technology_broker", "Guardian"),
        ] {
            let found = find(&c, &Want::Service { service: s }, sol(), &Filters::default(), 5, NOW).unwrap();
            assert_eq!(rec.bodies.lock().unwrap().last().unwrap()["filters"][field]["value"][0], kind);
            for h in &found.hits {
                let rec = all[s.id()]["results"].as_array().unwrap().iter().find(|r| r["name"] == h.name.as_str());
                assert_eq!(rec.unwrap()[field], kind, "{s:?}: {}", h.name);
            }
        }
        // The "any type" page holds traders of every type; asking for one type out of it keeps
        // only that type, even if Spansh ignored the type filter.
        let any = all["material_trader"].to_string();
        let (c, _) = client(Some(any));
        let raw = find(&c, &Want::Service { service: Service::RawTrader }, sol(), &Filters::default(), 5, NOW).unwrap();
        let names: Vec<&str> = raw.hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["Broglie Terminal"], "only the raw trader in the mixed page");
    }

    #[test]
    fn an_ignored_filter_gives_a_warning_not_wrong_stations() {
        // Spansh answering a black-market search with stations that have none.
        let (c, _) = client(Some(serde_json::from_str::<Value>(SERVICES).unwrap()["guardian_broker"].to_string()));
        let found =
            find(&c, &Want::Service { service: Service::BlackMarket }, sol(), &Filters::default(), 5, NOW).unwrap();
        assert!(found.hits.is_empty());
        assert!(found.warning.unwrap().contains("black market"));
    }

    #[test]
    fn filters_go_to_spansh_and_are_rechecked() {
        let (c, rec) = client(None);
        let strict = Filters {
            pad: Some(Pad::L),
            exclude_carriers: true,
            exclude_odyssey: true,
            max_arrival_ls: Some(1000),
            max_age_days: Some(7),
            ..Filters::default()
        };
        let names = |f: &Found| f.hits.iter().map(|h| h.name.clone()).collect::<Vec<_>>();
        // The recorded refuel page ignores the filters (three settlements, all far from the
        // star): the re-check keeps the two large-pad stations near it.
        let refuel = Want::Service { service: Service::Refuel };
        let found = find(&c, &refuel, sol(), &strict, 5, NOW).unwrap();
        assert_eq!(names(&found), ["Ehrlich City", "Furukawa Enterprise"]);
        assert_eq!(names(&find(&c, &refuel, sol(), &Filters::default(), 5, NOW).unwrap()).len(), 5);

        let body = rec.bodies.lock().unwrap().first().unwrap().clone();
        let f = &body["filters"];
        assert_eq!(f["services"][0]["name"][0], "Refuel");
        assert_eq!(f["has_large_pad"]["value"], true);
        let types: Vec<&str> = f["type"]["value"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert!(types.contains(&"Coriolis Starport") && types.contains(&"Mega ship"));
        assert!(!types.contains(&"Drake-Class Carrier") && !types.contains(&"Settlement"));
        assert_eq!(f["distance_to_arrival"]["value"][1], "1000");
        assert_eq!(f["updated_at"]["value"][0], "2026-10-09T12:00:00Z", "a week before now, on the hour");
        assert_eq!(body["size"], SEARCH_SIZE);

        // One filter at a time on the black-market page (a mega ship last seen in 2021, two
        // settlements, two carriers).
        let bm = Want::Service { service: Service::BlackMarket };
        let only = |f: Filters| names(&find(&c, &bm, sol(), &f, 5, NOW).unwrap());
        assert_eq!(
            only(Filters { exclude_carriers: true, ..Filters::default() }),
            ["Rescue Ship - Li Qing Jao", "Markus Extraction Exchange", "Kawle Genetics Complex"]
        );
        assert_eq!(
            only(Filters { exclude_odyssey: true, ..Filters::default() }),
            ["Rescue Ship - Li Qing Jao", "T8J-N7L", "V7T-33H"]
        );
        assert_eq!(
            only(Filters { max_age_days: Some(30), ..Filters::default() }),
            ["Markus Extraction Exchange", "V7T-33H", "Kawle Genetics Complex"]
        );
        assert_eq!(only(Filters { pad: Some(Pad::M), ..Filters::default() }).len(), 5, "an unknown pad is kept");
        assert_eq!(
            only(Filters { pad: Some(Pad::L), ..Filters::default() }),
            ["Rescue Ship - Li Qing Jao", "T8J-N7L", "V7T-33H", "Kawle Genetics Complex"]
        );
    }

    #[test]
    fn pad_comes_from_pad_counts() {
        let all: Value = serde_json::from_str(SERVICES).unwrap();
        let stations: Vec<Station> = serde_json::from_value(all["black_market"]["results"].clone()).unwrap();
        let pads: Vec<Option<Pad>> = stations.iter().map(pad_of).collect();
        assert_eq!(pads, [None, Some(Pad::M), Some(Pad::L), Some(Pad::L), Some(Pad::L)]);
    }

    /// Spansh's answers for a 5A Frame Shift Drive and a 3C Multi-Cannon near Sol (recorded
    /// 16 Oct 2026, trimmed to eight stations and the asked-for module).
    const MODULES: &str = include_str!("../tests/fixtures/spansh-modules.json");

    fn module_page(key: &str) -> String {
        serde_json::from_str::<Value>(MODULES).unwrap()[key].to_string()
    }

    #[test]
    fn modules_are_listed_nearest_first_with_price_and_honour_the_pad() {
        let (c, rec) = client(Some(module_page("fsd_5a")));
        let fsd = Want::Module(ModuleWant::from_words("Frame Shift Drive", "5a"));
        let all = find(&c, &fsd, sol(), &Filters::default(), 10, NOW).unwrap();
        assert_eq!(all.hits.len(), 8);
        assert!(all.hits.windows(2).all(|w| w[0].ly <= w[1].ly), "nearest first");
        assert_eq!(all.hits[0].name, "Abraham Lincoln");
        assert_eq!(all.hits[0].price, Some(5_103_950));
        assert_eq!(all.hits[0].detail.as_deref(), Some("5A Frame Shift Drive"));
        let body = rec.bodies.lock().unwrap().last().unwrap().clone();
        assert_eq!(
            body["filters"]["modules"],
            serde_json::json!([{ "name": ["Frame Shift Drive"], "class": ["5"], "rating": ["A"] }])
        );

        // A large-pad ship: the outposts (medium pads) go.
        let large = Filters { pad: Some(Pad::L), ..Filters::default() };
        let l = find(&c, &fsd, sol(), &large, 10, NOW).unwrap();
        assert!(l.hits.len() < all.hits.len());
        assert!(l.hits.iter().all(|h| h.pad == Some(Pad::L)), "{:?}", l.hits);
        assert!(!l.hits.iter().any(|h| h.station_type.as_deref() == Some("Outpost")));
        assert!(l.hits.windows(2).all(|w| w[0].ly <= w[1].ly));
    }

    #[test]
    fn the_mount_is_checked_on_each_station() {
        let (c, rec) = client(Some(module_page("multicannon_3c")));
        let any = Want::Module(ModuleWant::from_words("Multi-Cannon", "3c"));
        let gimballed = Want::Module(ModuleWant::from_words("Multi-Cannon", "3c gimballed"));
        let all = find(&c, &any, sol(), &Filters::default(), 10, NOW).unwrap();
        let g = find(&c, &gimballed, sol(), &Filters::default(), 10, NOW).unwrap();
        assert_eq!(all.hits.len(), 8);
        let names: Vec<&str> = g.hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["Galileo", "Mars High", "M.Gorbachev", "Li Qing Jao", "Columbus"]);
        assert_eq!(g.hits[0].price, Some(578_440));
        assert_eq!(g.hits[0].detail.as_deref(), Some("3C Multi-Cannon (gimballed)"));
        assert_eq!(all.hits[0].detail.as_deref(), Some("3C Multi-Cannon (fixed), 3C Multi-Cannon (gimballed)"));
        assert_eq!(all.hits[0].price, Some(140_400), "the cheapest fitting module");
        // Spansh can't filter the mount, so it isn't sent.
        let body = rec.bodies.lock().unwrap().last().unwrap().clone();
        assert_eq!(body["filters"]["modules"][0].as_object().unwrap().len(), 3);
        // A module the page doesn't have: nothing, with a warning.
        let rail = Want::Module(ModuleWant::from_words("Rail Gun", ""));
        let none = find(&c, &rail, sol(), &Filters::default(), 10, NOW).unwrap();
        assert!(none.hits.is_empty() && none.warning.is_some());
    }

    #[test]
    fn a_mount_none_of_the_page_sells_says_so_not_that_the_filter_changed() {
        // The recorded 3C Multi-Cannon page sells fixed and gimballed ones only; Spansh can't
        // filter the mount, so only the check here empties the page.
        let (c, _) = client(Some(module_page("multicannon_3c")));
        let turreted = Want::Module(ModuleWant::from_words("Multi-Cannon", "3c turreted"));
        let found = find(&c, &turreted, sol(), &Filters::default(), 10, NOW).unwrap();
        assert!(found.hits.is_empty());
        assert_eq!(
            found.warning.as_deref(),
            Some("None of the 8 nearest stations selling 3C Multi-Cannon sell it turreted.")
        );
    }

    #[test]
    fn modules_spansh_names_differently_are_asked_for_by_every_name() {
        // A recorded FSD page with one station's module renamed the way Spansh lists the
        // standard fighter bay ("Fighter Hangar", symbol Int_FighterBay_Size5_Class1).
        let mut page: Value = serde_json::from_str(&module_page("fsd_5a")).unwrap();
        page["results"][0]["modules"] = serde_json::json!([{
            "name": "Fighter Hangar", "class": 5, "rating": "D", "category": "internal",
            "ed_symbol": "Int_FighterBay_Size5_Class1", "price": 575_200
        }]);
        let (c, rec) = client(Some(page.to_string()));
        let hangar = Want::Module(ModuleWant::from_words("Vessel Hangar", "5d"));
        let found = find(&c, &hangar, sol(), &Filters::default(), 10, NOW).unwrap();
        assert_eq!(found.hits.len(), 1);
        assert_eq!((found.hits[0].name.as_str(), found.hits[0].price), ("Abraham Lincoln", Some(575_200)));
        let body = rec.bodies.lock().unwrap().last().unwrap().clone();
        assert_eq!(body["filters"]["modules"][0]["name"], serde_json::json!(["Vessel Hangar", "Fighter Hangar"]));
    }

    /// The recorded FSD page with its first two stations' modules replaced by `first` and
    /// `second`.
    fn page_selling(first: Value, second: Value) -> String {
        let mut page: Value = serde_json::from_str(&module_page("fsd_5a")).unwrap();
        page["results"][0]["modules"] = first;
        page["results"][1]["modules"] = second;
        page["results"].as_array_mut().unwrap().truncate(2);
        page.to_string()
    }

    #[test]
    fn armour_is_found_by_its_ship_symbols() {
        let (c, _) = client(Some(page_selling(
            serde_json::json!([{
                "name": "Military Grade Composite", "class": 1, "rating": "I", "category": "standard",
                "ship": "Adder", "ed_symbol": "Adder_Armour_Grade3", "price": 82_000
            }]),
            serde_json::json!([{
                "name": "Reinforced Alloy", "class": 1, "rating": "I", "category": "standard",
                "ship": "Adder", "ed_symbol": "Adder_Armour_Grade2", "price": 35_120
            }]),
        )));
        assert!(crate::outfitting::module("Military Grade Composite").is_some(), "armour is in the catalogue");
        let mgc = Want::Module(ModuleWant::from_words("Military Grade Composite", ""));
        let found = find(&c, &mgc, sol(), &Filters::default(), 10, NOW).unwrap();
        assert_eq!(found.hits.len(), 1);
        assert_eq!((found.hits[0].name.as_str(), found.hits[0].price), ("Abraham Lincoln", Some(82_000)));
    }

    #[test]
    fn a_pre_engineered_module_and_its_base_module_are_told_apart() {
        // Spansh gives a pre-engineered module its base module's journal symbol.
        let (c, _) = client(Some(page_selling(
            serde_json::json!([{
                "name": "Enduring Feedback Rail Gun", "class": 2, "rating": "B", "category": "hardpoint",
                "ed_symbol": "Hpt_Railgun_Fixed_Medium", "price": 412_800
            }]),
            serde_json::json!([{
                "name": "Rail Gun", "class": 2, "rating": "B", "category": "hardpoint",
                "ed_symbol": "Hpt_Railgun_Fixed_Medium", "price": 412_800
            }]),
        )));
        let names = |w: &Want| {
            let found = find(&c, w, sol(), &Filters::default(), 10, NOW).unwrap();
            found.hits.into_iter().map(|h| h.name).collect::<Vec<_>>()
        };
        let rail = Want::Module(ModuleWant::from_words("Rail Gun", "2b fixed"));
        let all: Value = serde_json::from_str(MODULES).unwrap();
        assert_eq!(names(&rail), [all["fsd_5a"]["results"][1]["name"].as_str().unwrap()]);
        let efr = Want::Module(ModuleWant::from_words("Enduring Feedback Rail Gun", "fixed"));
        assert!(crate::outfitting::module("Enduring Feedback Rail Gun").is_some(), "in the catalogue");
        assert_eq!(names(&efr), ["Abraham Lincoln"]);
    }

    #[test]
    fn module_age_is_the_outfitting_age() {
        let (c, rec) = client(Some(module_page("fsd_5a")));
        let fsd = Want::Module(ModuleWant::from_words("Frame Shift Drive", "5a"));
        let f = Filters { max_age_days: Some(3), ..Filters::default() };
        let found = find(&c, &fsd, sol(), &f, 10, NOW).unwrap();
        assert!(!found.hits.is_empty());
        let body = rec.bodies.lock().unwrap().last().unwrap().clone();
        assert!(body["filters"]["outfitting_updated_at"].is_object());
        assert!(body["filters"].get("updated_at").is_none());
        let all: Value = serde_json::from_str(MODULES).unwrap();
        for h in &found.hits {
            let r = all["fsd_5a"]["results"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["name"] == h.name.as_str() && r["system_name"] == h.system.as_str())
                .unwrap();
            assert_eq!(h.updated_at.as_deref(), r["outfitting_updated_at"].as_str());
        }
    }

    /// Spansh's answers for an Anaconda, a Krait MkII and a Type-9 near Sol (recorded 16 Oct
    /// 2026, trimmed to eight stations and the asked-for ship).
    const SHIPS: &str = include_str!("../tests/fixtures/spansh-ships.json");

    fn ship_page(key: &str) -> Value {
        serde_json::from_str::<Value>(SHIPS).unwrap()[key].clone()
    }

    #[test]
    fn shipyards_are_nearest_first_with_price() {
        let (c, rec) = client(Some(ship_page("krait_mkii").to_string()));
        let krait = Want::Ship { ship: "Krait MkII".into() };
        let found = find(&c, &krait, sol(), &Filters::default(), 10, NOW).unwrap();
        assert_eq!(found.hits.len(), 8);
        assert_eq!(found.hits[0].name, "Ehrlich City");
        assert_eq!(found.hits[0].price, Some(45_814_205));
        assert!(found.hits.windows(2).all(|w| w[0].ly <= w[1].ly), "nearest first");
        let body = rec.bodies.lock().unwrap().last().unwrap().clone();
        assert_eq!(body["filters"]["ships"], serde_json::json!([{ "name": ["Krait MkII"] }]));
        assert!(body["filters"].get("has_large_pad").is_none(), "a Krait needs only a medium pad");
        let all = ship_page("krait_mkii");
        let r = all["results"].as_array().unwrap().iter().find(|r| r["name"] == "Ehrlich City").unwrap();
        assert_eq!(found.hits[0].updated_at.as_deref(), r["shipyard_updated_at"].as_str(), "shipyard age");
        // a page without the ship: nothing, with a warning
        let mamba = find(&c, &Want::Ship { ship: "Mamba".into() }, sol(), &Filters::default(), 10, NOW).unwrap();
        assert!(mamba.hits.is_empty() && mamba.warning.is_some());
    }

    #[test]
    fn ships_too_big_for_a_stations_pads_are_left_out() {
        // The recorded Type-9 page, with Columbus standing in for a shipyard that has no large
        // pad (every recorded Type-9 shipyard has one, as in game).
        let mut page = ship_page("type9");
        for r in page["results"].as_array_mut().unwrap() {
            if r["name"] == "Columbus" {
                r["large_pads"] = 0.into();
                r["has_large_pad"] = false.into();
            }
        }
        let (c, rec) = client(Some(page.to_string()));
        let t9 = Want::Ship { ship: "Type-9 Heavy".into() };
        // Even with "any pad" picked, a Type-9 needs a large pad.
        let any = Filters { pad: None, pad_set: true, ..Filters::default() };
        let found = find(&c, &t9, sol(), &any, 10, NOW).unwrap();
        let names: Vec<&str> = found.hits.iter().map(|h| h.name.as_str()).collect();
        assert!(!names.contains(&"Columbus"), "{names:?}");
        assert_eq!(names.len(), 7);
        assert!(found.hits.iter().all(|h| h.pad == Some(Pad::L)));
        assert_eq!(rec.bodies.lock().unwrap().last().unwrap()["filters"]["has_large_pad"]["value"], true);
        // A Krait (medium) keeps a medium-only station, unless the commander's pad says large.
        assert_eq!(Want::Ship { ship: "Krait MkII".into() }.filters(&any).pad, Some(Pad::M));
        let large = Filters { pad: Some(Pad::L), ..Filters::default() };
        assert_eq!(Want::Ship { ship: "Krait MkII".into() }.filters(&large).pad, Some(Pad::L));
    }

    #[test]
    fn service_ids_round_trip() {
        for s in Service::ALL {
            assert_eq!(Service::from_id(&s.id()), Some(s));
        }
        assert_eq!(Service::InterstellarFactors.id(), "interstellar_factors");
        let w: Want = serde_json::from_str(r#"{"kind":"service","service":"guardian_broker"}"#).unwrap();
        assert_eq!(w, Want::Service { service: Service::GuardianBroker });
        let m: Want = serde_json::from_str(
            r#"{"kind":"module","name":"Multi-Cannon","class":3,"rating":"C","mount":"Gimballed"}"#,
        )
        .unwrap();
        assert_eq!(m.label(), "3C Multi-Cannon (gimballed)");
        let m: Want =
            serde_json::from_str(r#"{"kind":"module","name":"Fuel Scoop","class":null,"rating":null,"mount":null}"#)
                .unwrap();
        assert_eq!(m.label(), "Fuel Scoop");
    }
}
