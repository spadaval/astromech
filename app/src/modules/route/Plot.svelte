<script lang="ts">
  // Plot a route with Spansh for the current ship and follow it (docs/PLOTTING.md): neutron
  // (boosts from neutron stars; its stops are the neutron stars) or exact (every jump, with
  // refuel stops). The range and drive come from the ship's latest Loadout; Spansh works the
  // route out as a job, so it runs only on "Plot and follow" and can take a minute.
  import { plotRoute, plotSetup, type PlotSetup, type Plotted, type Plotter, type RouteView } from "../../lib/route";
  import OriginInput from "../../lib/ui/OriginInput.svelte";
  import type { Origin } from "../../lib/search";

  let { onfollow }: { onfollow: (v: RouteView) => void } = $props();

  let plotter = $state<Plotter>("neutron");
  let origin = $state<Origin>(null);
  let to = $state("");
  let efficiency = $state(60);
  let refuel = $state(true);
  let setup = $state<PlotSetup | null>(null);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let done = $state<Plotted | null>(null);

  $effect(() => {
    plotSetup()
      .then((s) => (setup = s))
      .catch((e) => (setup = { here: null, ship: null, ship_error: String(e instanceof Error ? e.message : e) }));
  });

  const ship = $derived(setup?.ship ?? null);
  const ready = $derived(!!ship && !!to.trim() && (origin != null || !!setup?.here) && !busy);

  async function go() {
    busy = true;
    err = null;
    done = null;
    try {
      const eff = Math.min(100, Math.max(1, Math.round(Number(efficiency) || 60)));
      const r = await plotRoute({ plotter, from: origin ?? "", to: to.trim(), efficiency: eff, refuel_every_scoopable: refuel });
      done = r.plotted;
      onfollow(r.view);
    } catch (e) {
      err = String(e instanceof Error ? e.message : e);
    } finally {
      busy = false;
    }
  }

  const ly = (n: number) => Math.round(n).toLocaleString("en-GB");
  const count = (n: number, one: string, many: string) => `${n.toLocaleString("en-GB")} ${n === 1 ? one : many}`;
  function summary(p: Plotted): string {
    const parts = [count(p.jumps, "jump", "jumps"), `${ly(p.ly)} ly`];
    if (p.neutrons) parts.push(count(p.neutrons, "neutron boost", "neutron boosts"));
    if (p.refuels) parts.push(count(p.refuels, "refuel stop", "refuel stops"));
    return parts.join(" · ");
  }
</script>

<div class="plot">
  <h4>Plot with Spansh</h4>
  <div class="kinds" role="group" aria-label="Plotter">
    <button class:on={plotter === "neutron"} aria-pressed={plotter === "neutron"} onclick={() => (plotter = "neutron")}>Neutron</button>
    <button class:on={plotter === "exact"} aria-pressed={plotter === "exact"} onclick={() => (plotter = "exact")}>Exact</button>
  </div>
  <OriginInput {origin} onchange={(o) => (origin = o)} />
  <div class="fields">
    <label><span>To</span><input aria-label="Destination system" placeholder="System name" spellcheck="false" bind:value={to} /></label>
    {#if plotter === "neutron"}
      <label><span>Efficiency</span><input aria-label="Efficiency" type="number" min="1" max="100" bind:value={efficiency} /><em>%</em></label>
    {:else}
      <label class="check"><input type="checkbox" bind:checked={refuel} /> Refuel at every scoopable star</label>
    {/if}
  </div>
  {#if ship}
    <p class="lo">
      {ship.name}: {ship.range.toFixed(2)} ly a jump with a full tank{ship.cargo ? ` and ${Math.round(ship.cargo)} t of cargo` : ""}, from its last loadout.
      {#if ship.unresolved.length}Some modules are not counted, so the range may be off.{/if}
    </p>
  {:else if setup?.ship_error}
    <p class="lo">{setup.ship_error}</p>
  {/if}
  <p class="lo">
    {plotter === "neutron"
      ? "Stops at the neutron stars to boost from. Plot each leg in the galaxy map."
      : "Every jump is a waypoint. Waypoints marked fuel are where to scoop."}
  </p>
  <button class="go" disabled={!ready} onclick={() => void go()}>{busy ? "Plotting…" : "Plot and follow"}</button>
  {#if busy}
    <p class="lo">Asking Spansh. A route can take a minute.</p>
  {:else if err}
    <p class="err">{err}</p>
  {:else if done}
    <p class="ok">Following {done.name}: {summary(done)}.</p>
  {/if}
</div>

<style>
  .plot { display: flex; flex-direction: column; gap: 4px; }
  h4 { margin: 0 0 6px; color: var(--text-hi); font-weight: 500; text-transform: uppercase; letter-spacing: var(--caps); font-size: var(--fs-s); }
  p { margin: 0; font-size: var(--fs-s); }
  button { all: unset; cursor: pointer; box-sizing: border-box; }
  .kinds { display: flex; gap: 18px; margin: 0 0 8px; font-size: var(--fs-s); text-transform: uppercase; letter-spacing: var(--caps); }
  .kinds button { color: var(--text-lo); }
  .kinds button:hover { color: var(--text); }
  .kinds .on { color: var(--accent); }
  .fields { display: flex; flex-direction: column; gap: 4px; margin: 0 0 6px; font-size: var(--fs-s); }
  label { display: flex; align-items: center; gap: 8px; }
  label > span { color: var(--text-lo); text-transform: uppercase; letter-spacing: var(--caps); min-width: 5.5em; }
  label em { font-style: normal; color: var(--text-lo); min-width: 1.5em; }
  input:not([type="checkbox"]) { all: unset; flex: 1; min-width: 0; height: 28px; padding: 0 8px; background: var(--cell-opt); color: var(--text-hi); font-family: var(--mono); }
  .plot :global(.origin .lbl) { min-width: 5.5em; }
  input::placeholder { color: var(--text-lo); }
  .check { color: var(--text); }
  .go { align-self: flex-start; margin: 6px 0 4px; padding: 6px 14px; background: var(--cell); color: var(--accent); text-transform: uppercase; letter-spacing: var(--caps); font-size: var(--fs-s); }
  .go:disabled { color: var(--text-lo); cursor: default; }
  .lo { color: var(--text-lo); }
  .ok { color: var(--accent); }
  .err { color: var(--text-hi); background: var(--accent); padding: 8px 12px; }
</style>
