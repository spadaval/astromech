import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import KeybindsView from "../../modules/keybinds/KeybindsView.svelte";
import PresetsView from "../../modules/presets/View.svelte";
import PresetPicker from "./PresetPicker.svelte";
import { binds } from "../store.svelte";
import { api, type State } from "../binds";

// Each page shows exactly one of loading, empty, error or content. The browser fallback serves the
// sample preset; these tests swap what `api.state` / `api.view` answer.
Element.prototype.scrollIntoView = () => {};
const NONE = "No keybindings found. Run Elite once, or set the Bindings folder in Settings.";
const NO_PRESETS = "No presets found. Run Elite once, or set the Bindings folder in Settings.";
const never = () => new Promise<never>(() => {});

let sample: State;
beforeEach(async () => {
  sample ??= await api.state();
  vi.restoreAllMocks();
  binds.start(); // the first call loads once with the real fallback; let that settle, then reset
  await binds.load();
  await new Promise((r) => setTimeout(r, 0));
  binds.state = null;
  binds.view = null;
  binds.error = null;
  localStorage.clear();
});
afterEach(cleanup);

/** Load with what the spies answer (they must be set first), then show the page. */
async function show(page: Parameters<typeof render>[0]) {
  await binds.load();
  return render(page);
}
const empty = (): State => ({ ...structuredClone(sample), presets: [], active: [] });

describe("All keybinds states", () => {
  it("says loading while the state is pending", async () => {
    vi.spyOn(api, "state").mockImplementation(never);
    binds.state = null;
    render(KeybindsView);
    expect(screen.getByText("Loading…")).toBeTruthy();
    expect(screen.queryByText(NONE)).toBeNull();
  });

  it("says nothing was found when there are no presets", async () => {
    vi.spyOn(api, "state").mockResolvedValue(empty());
    await show(KeybindsView);
    expect(screen.getByText(NONE)).toBeTruthy();
    expect(screen.queryByText("Loading…")).toBeNull();
  });

  it("is not 'nothing found' while the view of a known preset is still loading", async () => {
    vi.spyOn(api, "state").mockResolvedValue(structuredClone(sample));
    vi.spyOn(api, "view").mockImplementation(never);
    binds.state = structuredClone(sample); // as load() leaves it between its two awaits
    render(KeybindsView);
    expect(screen.getByText("Loading…")).toBeTruthy();
    expect(screen.queryByText(NONE)).toBeNull();
  });

  it("shows a failed state call once, with no loading or empty text", async () => {
    vi.spyOn(api, "state").mockRejectedValue(new Error("core is down"));
    await show(KeybindsView);
    expect(screen.getAllByText(/core is down/)).toHaveLength(1);
    expect(screen.queryByText("Loading…")).toBeNull();
    expect(screen.queryByText(NONE)).toBeNull();
  });

  it("shows a failed view call once, with no loading or empty text", async () => {
    vi.spyOn(api, "state").mockResolvedValue(structuredClone(sample));
    vi.spyOn(api, "view").mockRejectedValue(new Error("bindings file unreadable"));
    await show(KeybindsView);
    expect(screen.getAllByText(/bindings file unreadable/)).toHaveLength(1);
    expect(screen.queryByText("Loading…")).toBeNull();
    expect(screen.queryByText(NONE)).toBeNull();
  });

  it("lists the groups when data is present", async () => {
    await show(KeybindsView);
    await screen.findByText("Flight", { selector: ".gname" });
    expect(screen.queryByText("Loading…")).toBeNull();
    expect(screen.queryByText(NONE)).toBeNull();
  });
});

describe("Presets states", () => {
  it("says no presets when there are none", async () => {
    vi.spyOn(api, "state").mockResolvedValue(empty());
    await show(PresetsView);
    expect(screen.getByText(NO_PRESETS)).toBeTruthy();
  });

  it("shows a failed load instead of loading forever", async () => {
    vi.spyOn(api, "state").mockRejectedValue(new Error("core is down"));
    await show(PresetsView);
    expect(screen.getByText(/core is down/)).toBeTruthy();
    expect(screen.queryByText("Loading…")).toBeNull();
    expect(screen.queryByText(NO_PRESETS)).toBeNull();
  });

  it("says loading while pending", () => {
    vi.spyOn(api, "state").mockImplementation(never);
    render(PresetsView);
    expect(screen.getByText("Loading…")).toBeTruthy();
  });
});

describe("Preset picker", () => {
  it("offers 'No presets found' only once loaded with none", async () => {
    vi.spyOn(api, "state").mockImplementation(never);
    const view = render(PresetPicker);
    expect(view.container.querySelector("option")).toBeNull();
    cleanup();
    vi.restoreAllMocks();
    vi.spyOn(api, "state").mockResolvedValue(empty());
    await show(PresetPicker);
    expect(screen.getByText("No presets found")).toBeTruthy();
  });

  it("lists presets when present", async () => {
    await show(PresetPicker);
    expect(screen.queryByText("No presets found")).toBeNull();
  });
});
