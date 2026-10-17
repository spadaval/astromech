// Ship detail component tests. The stats come from the fixture astromech-ship's dump-ships-fixture
// example writes (lib/fixtures/ships.json): the synthetic Krait MkII combat build and its stats
// for each build plan. The expected numbers are what Coriolis shows for those builds, the same
// values crates/astromech-ship/tests/coriolis.rs checks (krait_mkii_combat and plan_*); a change
// is the planned value minus the current one, both as shown.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import ships from "../../lib/fixtures/ships.json";
import type { Compared, Fleet, Plan, StatsComparison } from "../../lib/engineering";
import Build from "./Build.svelte";

const asked: Plan[] = [];
/** Answer every plan with this instead of the fixture. */
let override: Compared | null = null;

vi.mock("../../lib/engineering", async (importActual) => {
  const real = await importActual<typeof import("../../lib/engineering")>();
  return {
    ...real,
    // The shell's engineering_stats, answered from the fixture: a plan it doesn't have fails.
    shipStats: async (plan: Plan): Promise<Compared | null> => {
      asked.push(plan);
      if (override) return override;
      if (plan.ship_id !== ships.fleet.ships[0]!.id) return null;
      const c = ships.cases.find((c) => real.samePlan(c.plan as unknown as Plan, plan));
      if (!c) throw new Error(`no fixture stats for ${JSON.stringify(plan.targets)}`);
      return c.stats as unknown as StatsComparison;
    },
  };
});

const fleet = ships.fleet as unknown as Fleet;
const krait = fleet.ships[0]!.id;

/** Now, Plan and Change for one stat. */
function stat(group: string, label: string): string[] {
  const g = screen.getByRole("rowgroup", { name: group });
  const th = within(g).getByRole("rowheader", { name: label });
  return [...th.parentElement!.querySelectorAll("td")].map((td) => td.textContent ?? "");
}

async function pick(label: string, value: string) {
  await fireEvent.change(screen.getByRole("combobox", { name: label }), { target: { value } });
}

beforeEach(() => {
  localStorage.clear();
  asked.length = 0;
  override = null;
});
afterEach(cleanup);

describe("ship detail stats", () => {
  it("shows current stats as the plan until the plan changes", async () => {
    render(Build, { fleet, shipId: krait, onback: () => {} });
    await waitFor(() => expect(stat("Distributor", "WEP capacity (MJ)")).toEqual(["92.2", "92.2", ""]));
    expect(stat("Jump range (ly)", "Max")).toEqual(["24.87", "24.87", ""]);
    expect(stat("Shields", "Strength (MJ)")).toEqual(["1428", "1428", ""]);
    expect(stat("Shields", "Thermal")).toEqual(["36.2", "36.2", ""]);
    expect(stat("Distributor", "Time to drain")).toEqual(["1:10", "1:10", ""]);
    expect(stat("Weapons and hull", "DPS")).toEqual(["107.2", "107.2", ""]);
    expect(screen.queryByText(/Incomplete/)).toBeNull();
  });

  it("switching the distributor experimental changes drain against recharge", async () => {
    render(Build, { fleet, shipId: krait, onback: () => {} });
    await waitFor(() => screen.getByRole("rowgroup", { name: "Distributor" }));
    // Weapon focused G5 with the fitted Super Conduits, rolled to 100 % (plan_pd_super_conduits).
    await pick("Power distributor blueprint", "PowerDistributor_PriorityWeapons");
    await waitFor(() => expect(stat("Distributor", "WEP capacity (MJ)")).toEqual(["92.2", "93.7", "+1.5"]));
    expect(stat("Distributor", "WEP recharge (MW)")).toEqual(["9.0", "9.1", "+0.1"]);
    expect(stat("Distributor", "Time to drain")).toEqual(["1:10", "1:18", "+0:08"]);
    // Then Cluster Capacitors (plan_pd_cluster_capacitors): more capacity, slower recharge.
    await pick("Power distributor experimental", "special_powerdistributor_capacity");
    await waitFor(() => expect(stat("Distributor", "WEP capacity (MJ)")).toEqual(["92.2", "105.4", "+13.2"]));
    expect(stat("Distributor", "WEP recharge (MW)")).toEqual(["9.0", "8.6", "-0.4"]);
    expect(stat("Distributor", "Time to drain")).toEqual(["1:10", "1:01", "-0:09"]);
    expect(stat("Distributor", "SYS recharge (MW)")).toEqual(["4.0", "3.7", "-0.3"]);
    expect(asked.at(-1)!.targets).toEqual([
      { slot: "PowerDistributor", blueprint: "PowerDistributor_PriorityWeapons", grade: 5, experimental: "special_powerdistributor_capacity" },
    ]);
  });

  it("switching the FSD blueprint changes the jump range", async () => {
    render(Build, { fleet, shipId: krait, onback: () => {} });
    await waitFor(() => screen.getByRole("rowgroup", { name: "Jump range (ly)" }));
    // Faster boot sequence G5, keeping Mass Manager (plan_fsd_fast_boot).
    await pick("Frame shift drive blueprint", "FSD_FastBoot");
    await waitFor(() => expect(stat("Jump range (ly)", "Max")).toEqual(["24.87", "18.71", "-6.16"]));
    expect(stat("Jump range (ly)", "Full tank")).toEqual(["24.05", "18.09", "-5.96"]);
    expect(stat("Jump range (ly)", "Laden")).toEqual(["23.15", "17.41", "-5.74"]);
    expect(stat("Jump range (ly)", "Total")).toEqual(["163.74", "123.19", "-40.55"]);
    expect(stat("Ship", "Mass (t)")).toEqual(["820", "814", "-6"]);
  });

  it("each thermal-resistant booster raises thermal resistance and lowers shield strength", async () => {
    render(Build, { fleet, shipId: krait, onback: () => {} });
    await waitFor(() => screen.getByRole("rowgroup", { name: "Shields" }));
    // Three thermal-resistant boosters instead of two (plan_thermal_boosters_3)...
    await pick("Utility 1 blueprint", "ShieldBooster_Thermic");
    await waitFor(() => expect(stat("Shields", "Thermal")).toEqual(["36.2", "43.9", "+7.7"]));
    expect(stat("Shields", "Strength (MJ)")).toEqual(["1428", "1197", "-231"]);
    expect(stat("Shields", "Kinetic")).toEqual(["43.4", "41.2", "-2.2"]);
    // ...then four (plan_thermal_boosters_4).
    await pick("Utility 2 blueprint", "ShieldBooster_Thermic");
    await waitFor(() => expect(stat("Shields", "Thermal")).toEqual(["36.2", "49.5", "+13.3"]));
    expect(stat("Shields", "Strength (MJ)")).toEqual(["1428", "967", "-461"]);
    expect(stat("Shields", "Explosive")).toEqual(["52.9", "49.0", "-3.9"]);
  });

  it("marks the ship incomplete when the calculations leave something out", async () => {
    const c = structuredClone(ships.cases[0]!.stats) as unknown as StatsComparison;
    c.current.unresolved = ["MainEngines: int_engine_size9_class9"];
    c.planned.unresolved = ["MainEngines: int_engine_size9_class9"];
    override = c;
    render(Build, { fleet, shipId: krait, onback: () => {} });
    const warn = await screen.findByText(/Incomplete: 1 item isn't in the ship data/);
    expect(warn.getAttribute("title")).toBe("MainEngines: int_engine_size9_class9");
  });
});

describe("ship detail subtitle", () => {
  it("shows where a stored ship is as a system chip", async () => {
    const stored = structuredClone(fleet);
    Object.assign(stored.ships[0]!, { current: false, system: "Shinrarta Dezhra" });
    render(Build, { fleet: stored, shipId: krait, onback: () => {} });
    const copy = await screen.findByRole("button", { name: "Copy Shinrarta Dezhra" });
    expect(copy.closest(".sub")!.textContent).toMatch(/stored at\s+Shinrarta Dezhra/);
  });
});

describe("ship detail link-outs", () => {
  it("opens the loadout in Coriolis and EDSY, and the system on Spansh and Inara", async () => {
    const opened = vi.spyOn(window, "open").mockImplementation(() => null);
    const stored = structuredClone(fleet);
    Object.assign(stored.ships[0]!, { current: false, system: "Shinrarta Dezhra" });
    const links = stored.ships[0]!.links!;
    render(Build, { fleet: stored, shipId: krait, onback: () => {} });
    await fireEvent.click(await screen.findByRole("button", { name: "Coriolis" }));
    await fireEvent.click(screen.getByRole("button", { name: "EDSY" }));
    await fireEvent.click(screen.getByRole("button", { name: "Spansh" }));
    await fireEvent.click(screen.getByRole("button", { name: "Inara" }));
    expect(opened.mock.calls.map((c) => c[0])).toEqual([
      links.coriolis,
      links.edsy,
      "https://spansh.co.uk/search/Shinrarta%20Dezhra",
      "https://inara.cz/elite/starsystem/?search=Shinrarta%20Dezhra",
    ]);
    expect(links.coriolis).toMatch(/^https:\/\/coriolis\.io\//);
    opened.mockRestore();
  });

  it("says to refresh the loadout when the saved one is stale", async () => {
    const stale = structuredClone(fleet);
    delete stale.ships[0]!.links;
    render(Build, { fleet: stale, shipId: krait, onback: () => {} });
    expect(await screen.findByText("Re-dock or relog to refresh the Coriolis / EDSY link")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Coriolis" })).toBeNull();
  });
});

describe("ship detail with an unknown hull", () => {
  it("shows the ship as incomplete instead of an error", async () => {
    override = { unknown_hull: "mediumtransport01" };
    render(Build, { fleet, shipId: krait, onback: () => {} });
    const warn = await screen.findByText(/Incomplete: this hull isn't in the ship data/);
    expect(warn.getAttribute("title")).toBe("Hull: mediumtransport01");
    expect(screen.queryByRole("rowgroup", { name: "Shields" })).toBeNull();
  });
});

describe("ship detail panels", () => {
  it("still shows the shopping list, trades or farming, the engineer trip and material details", async () => {
    const { loadFleet } = await import("../../lib/engineering");
    const sample = await loadFleet();
    // The sample journal's Krait (id 7) and its sample plan, as on the Builds page.
    render(Build, { fleet: sample, shipId: 7, onback: () => {} });
    await screen.findByRole("heading", { name: "Shopping list" });
    expect(screen.getByRole("heading", { name: "Trade at a material trader" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Still to collect" })).toBeTruthy();
    await screen.findByRole("heading", { name: "Engineer trip" });
    expect(screen.getByText("Ship stats are worked out in the app. Outside it, only the sample Krait's sample plans have them.")).toBeTruthy();
    const first = document.querySelector<HTMLButtonElement>("button.mrow")!;
    const name = first.querySelector(".mn")!.textContent!.replace(/^G\d /, "").replace(" ⚠", "");
    await fireEvent.click(first);
    expect(screen.getByRole("heading", { level: 3, name })).toBeTruthy();
    expect(screen.getByText(/^Used by:/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "← Ship stats" }));
    expect(screen.getByRole("heading", { level: 3, name: "Engineering" })).toBeTruthy();
  });
});

describe("where to buy the plan's modules", () => {
  /** The search kit's rows: station, system, the distance line and the facts line. */
  function kitRows() {
    return [...document.querySelectorAll(".kit li")].map((li) => ({
      station: li.querySelector(".sys")?.firstChild?.textContent?.replace(/ · $/, ""),
      system: li.querySelector(".sys .name")?.textContent,
      q: li.querySelector(".q")?.textContent,
      facts: li.querySelector(":scope > .lo")?.textContent ?? "",
    }));
  }

  /** The sample Krait (id 7) with a plan for its FSD, thrusters, shield booster and armour. */
  async function threeModulePlan() {
    const { loadFleet, savePlan } = await import("../../lib/engineering");
    savePlan({
      ship_id: 7,
      rolls: [1, 2, 3, 4, 5],
      targets: [
        { slot: "FrameShiftDrive", blueprint: "FSD_LongRange", grade: 5, experimental: null },
        { slot: "MainEngines", blueprint: "Engine_Dirty", grade: 5, experimental: null },
        { slot: "TinyHardpoint1", blueprint: "ShieldBooster_HeavyDuty", grade: 5, experimental: null },
        { slot: "Armour", blueprint: "Armour_HeavyDuty", grade: 5, experimental: null },
      ],
    });
    return await loadFleet();
  }

  it("lists stations selling the most of the plan's modules first, then nearest", async () => {
    render(Build, { fleet: await threeModulePlan(), shipId: 7, onback: () => {} });
    await screen.findByRole("heading", { name: "Where to buy the plan's modules" });
    // In the ship's slot order; armour is sold per ship, so it isn't searched.
    expect(screen.getByText(/^6A Thrusters, 5A Frame Shift Drive, 0A Shield Booster \(not searched: Lightweight Alloy\)\./)).toBeTruthy();
    await waitFor(() => expect(kitRows().length).toBeGreaterThan(1));
    const rows = kitRows();
    // A station selling all three (4.4 ly) above a closer one selling two (Sol, 0 ly).
    expect(rows[0]).toMatchObject({ station: "Stronghold Carrier", system: "Alpha Centauri" });
    expect(rows[0]?.q).toMatch(/^3 of 3 · 1 jump · 4\.4 ly$/);
    const lincoln = rows.findIndex((r) => r.station === "Abraham Lincoln");
    expect(lincoln).toBeGreaterThan(0);
    expect(rows[lincoln]?.q).toMatch(/^2 of 3 · .* 0\.0 ly$/);
    expect(rows[lincoln]?.facts).toContain("no 6A Thrusters");
  });

  it("follows the plan: a module dropped from it drops from the search", async () => {
    render(Build, { fleet: await threeModulePlan(), shipId: 7, onback: () => {} });
    await screen.findByText(/^6A Thrusters, 5A Frame Shift Drive, 0A Shield Booster/);
    await pick("Thrusters blueprint", "");
    await screen.findByText(/^5A Frame Shift Drive, 0A Shield Booster \(not searched/);
    await waitFor(() => expect(kitRows()[0]).toMatchObject({ station: "Abraham Lincoln", system: "Sol" }));
    expect(kitRows()[0]?.q).toMatch(/^2 of 2 · /);
  });

  it("isn't shown without a plan", async () => {
    const { loadFleet, savePlan } = await import("../../lib/engineering");
    savePlan({ ship_id: 7, rolls: [1, 2, 3, 4, 5], targets: [] });
    render(Build, { fleet: await loadFleet(), shipId: 7, onback: () => {} });
    await screen.findByRole("combobox", { name: "Thrusters blueprint" });
    expect(screen.queryByRole("heading", { name: "Where to buy the plan's modules" })).toBeNull();
  });
});
