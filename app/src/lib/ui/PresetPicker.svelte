<script lang="ts">
  // Which preset the keybinds screens show. Defaults to the one the game uses for the ship.
  import { binds } from "../store.svelte";
  const active = (ctx: string[]) => (ctx.length === 4 ? "active everywhere" : ctx.length ? `active: ${ctx.join(", ").toLowerCase()}` : "not active");
</script>

<label class="row">
  <span class="lbl">Preset</span>
  <select value={binds.view?.path ?? ""} onchange={(e) => void binds.show(e.currentTarget.value)}>
    {#if binds.state && !binds.state.presets.length}<option value="">No presets found</option>{/if}
    {#each binds.state?.presets ?? [] as p (p.path)}
      <option value={p.path}>{p.name}{p.source === "built-in" ? " (built-in)" : ""} · {active(p.active_for)}</option>
    {/each}
  </select>
</label>

<style>
  .row { display: grid; grid-template-columns: 160px 1fr; align-items: center; margin-top: 12px; }
  .lbl { padding-left: 34px; color: var(--text-lo); }
  select { all: unset; height: var(--row); padding: 0 12px; background: var(--cell); color: var(--accent); cursor: pointer; }
  option { background: var(--bg); color: var(--text); }
</style>
