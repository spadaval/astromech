import { describe, expect, it } from "vitest";
import { DEFAULT_FILTERS } from "./search";
import { moduleLabel, modulesNearby, modulesResults, stations, toResults, type ModuleWant } from "./stations";

describe("station lookups (browser preview)", () => {
  it("adapts the answer into the kit's rows, nearest first", async () => {
    const r = toResults(await stations({ kind: "service", service: "black_market" }, null, DEFAULT_FILTERS));
    expect(r.origin).toBe("Sol");
    expect(r.rows[0]).toMatchObject({ station: "Ehrlich City", system: "Sol", pad: "L", jumps: 0 });
    expect(r.rows[0]?.detail).toBe("Planetary Port");
    expect(r.rows.every((row, i) => i === 0 || (r.rows[i - 1]?.ly ?? 0) <= row.ly)).toBe(true);
  });

  it("applies the filters the way the shell does", async () => {
    const want = { kind: "service", service: "black_market" } as const;
    const all = await stations(want, "Alpha Centauri", DEFAULT_FILTERS);
    const some = await stations(want, "Alpha Centauri", { ...DEFAULT_FILTERS, exclude_carriers: true, exclude_odyssey: true, max_arrival_ls: 1000 });
    expect(some.here.system).toBe("Alpha Centauri");
    expect(some.hits.length).toBeLessThan(all.hits.length);
    expect(some.hits.some((h) => h.station_type === "Drake-Class Carrier" || h.station_type === "Settlement")).toBe(false);
    expect(some.hits.every((h) => h.arrival_ls == null || h.arrival_ls <= 1000)).toBe(true);
  });
});

describe("where to buy a list of modules (browser preview)", () => {
  it("labels a module as the core does", () => {
    expect(moduleLabel({ name: "Frame Shift Drive", class: 5, rating: "A", mount: null })).toBe("5A Frame Shift Drive");
    expect(moduleLabel({ name: "Shield Booster", class: 0, rating: "A", mount: null })).toBe("0A Shield Booster");
    expect(moduleLabel({ name: "Multi-Cannon", class: 3, rating: "C", mount: "Gimballed" })).toBe("3C Multi-Cannon (gimballed)");
    expect(moduleLabel({ name: "Fuel Scoop", class: 5, rating: null, mount: null })).toBe("class 5 Fuel Scoop");
    expect(moduleLabel({ name: "Frame Shift Drive", class: null, rating: "A", mount: null })).toBe("A-rated Frame Shift Drive");
  });

  const fsd: ModuleWant = { name: "Frame Shift Drive", class: 5, rating: "A", mount: null };
  const thrusters: ModuleWant = { name: "Thrusters", class: 6, rating: "A", mount: null };
  const booster: ModuleWant = { name: "Shield Booster", class: 0, rating: "A", mount: null };

  it("ranks a station selling all three above a closer one selling two", async () => {
    const r = await modulesNearby([fsd, thrusters, booster], null, DEFAULT_FILTERS);
    const names = r.stations.map((s) => s.name);
    // Abraham Lincoln (Sol, 0 ly) has no 6A Thrusters; Stronghold Carrier (4.4 ly) sells all three.
    const lincoln = r.stations.find((s) => s.name === "Abraham Lincoln");
    expect(names[0]).toBe("Stronghold Carrier");
    expect(r.stations[0]?.sold.map((x) => x.module)).toEqual(["5A Frame Shift Drive", "6A Thrusters", "0A Shield Booster"]);
    expect(lincoln === undefined || (lincoln.sold.length === 2 && names.indexOf("Abraham Lincoln") > 0)).toBe(true);
    expect(r.stations.every((s, i) => i === 0 || r.stations[i - 1]!.sold.length > s.sold.length || r.stations[i - 1]!.ly <= s.ly)).toBe(true);
    const rows = modulesResults(r, 3).rows;
    expect(rows[0]).toMatchObject({ station: "Stronghold Carrier", lead: "3 of 3" });
    expect(rows[0]?.detail).toMatch(/^Planetary Outpost · 5A Frame Shift Drive [\d,]+ cr, 6A Thrusters [\d,]+ cr, 0A Shield Booster [\d,]+ cr$/);
  });

  it("narrows to the listed modules, names what isn't sold and applies the filters", async () => {
    const two = await modulesNearby([fsd, booster], null, DEFAULT_FILTERS);
    expect(two.stations[0]).toMatchObject({ name: "Abraham Lincoln", missing: [] });
    expect(two.stations.every((s) => s.sold.every((x) => x.module !== "6A Thrusters"))).toBe(true);
    const odd = await modulesNearby([fsd, { name: "Rail Gun", class: 2, rating: "B", mount: "Fixed" }], "Alpha Centauri", DEFAULT_FILTERS);
    expect(odd.here.system).toBe("Alpha Centauri");
    expect(odd.unavailable).toEqual(["2B Rail Gun (fixed)"]);
    expect(modulesResults(odd, 2).warning).toBe("No station found sells 2B Rail Gun (fixed).");
    expect(modulesResults(odd, 2).rows[0]?.detail).toContain("no 2B Rail Gun (fixed)");
    const noCarriers = await modulesNearby([fsd, thrusters, booster], null, { ...DEFAULT_FILTERS, exclude_carriers: true, pad: "L" });
    expect(noCarriers.stations.some((s) => s.station_type === "Drake-Class Carrier" || s.pad !== "L")).toBe(false);
  });
});
