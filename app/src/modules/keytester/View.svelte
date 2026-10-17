<script lang="ts">
  // Key tester: every press from every stick, pad and key, with its Elite name, the VKB control
  // it should be, and what the shown preset binds to it. Read-only.
  import { binds } from "../../lib/store.svelte";
  import { controlFor } from "../../lib/devices";
  import { control } from "../../lib/format";

  binds.start();
  const ctl = (d: string, k: string) => (binds.state ? controlFor(d, k, binds.state.overrides)?.control : undefined);
</script>

<section>
  <p class="lo">Press anything. Each press shows the name Elite uses and what {binds.view?.name ?? "the preset"} does with it. Axes show when they settle.</p>
  <div class="grid">
    <div>
      <div class="head"><span>Control</span><span>Elite name</span><span>Bound to</span></div>
      {#each binds.log as p, i (p.t_ms + p.key + i)}
        {@const rows = binds.rowsFor(p.device, p.key)}
        <div class="row" class:new={i === 0}>
          <span>{control(p.device, p.key)}{#if ctl(p.device, p.key)}<span class="lo"> · {ctl(p.device, p.key)}</span>{/if}</span>
          <span class="mono">{p.device} {p.key}</span>
          <span>{rows.map((r) => r.name).join(", ") || "—"}</span>
        </div>
      {:else}
        <div class="row empty"><span class="lo">Waiting for a press…</span></div>
      {/each}
    </div>
    <div>
      <div class="head"><span>Connected controllers</span></div>
      {#each binds.state?.inventory.controllers ?? [] as c (c.elite_id)}
        <div class="dev"><span>{c.product.trim()}</span><span class="mono">{c.elite_id}</span><span class="lo">{c.buttons} buttons, {c.axes.length} axes, {c.hats} hat{c.hats === 1 ? "" : "s"}</span></div>
      {:else}
        <p class="lo">None found.</p>
      {/each}
      {#if (binds.state?.inventory.controllers ?? []).some((c) => c.buttons > 32)}
        <p class="lo">Elite reads only buttons 1-32 on each device.</p>
      {/if}
    </div>
  </div>
</section>

<style>
  section { padding: 28px 0; }
  p { margin: 0; }
  .grid { display: grid; grid-template-columns: minmax(0, 1fr) 380px; gap: 40px; margin-top: 20px; }
  .head, .row { display: grid; grid-template-columns: 260px 240px 1fr; gap: var(--gap); }
  .head { color: var(--text-lo); font-size: var(--fs-s); text-transform: uppercase; letter-spacing: var(--caps); padding-bottom: 6px; }
  .row { min-height: 30px; margin-bottom: var(--gap); }
  .row > span { background: var(--cell-empty); padding: 0 12px; display: flex; align-items: center; }
  .row.new > span { background: var(--cell); color: var(--text-hi); }
  .row.empty { grid-template-columns: 1fr; }
  .dev { display: grid; gap: 2px; padding: 8px 12px; background: var(--cell-opt); margin-bottom: var(--gap); }
  .mono { font-family: var(--mono); font-size: var(--fs-s); }
  .lo { color: var(--text-lo); }
  @media (max-width: 1200px) {
    .grid { grid-template-columns: minmax(0, 1fr) 280px; gap: 24px; }
    .head, .row { grid-template-columns: 150px 150px minmax(0, 1fr); }
  }
</style>
