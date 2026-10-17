<script lang="ts">
  // Presets (docs/V1-DESIGN.md §2.3): every preset in the Bindings folder and the game's built-in
  // ones, which is active per context (StartPreset), the health check's problems, copies,
  // switching, and astromech's backups with restore.
  import { binds } from "../../lib/store.svelte";
  import { api, CONTEXTS, type Preset } from "../../lib/binds";

  binds.start();
  let sel = $state<string | null>(null);
  let copyName = $state("");
  let msg = $state<string | null>(null);
  const p = $derived(binds.state?.presets.find((x) => x.path === sel) ?? binds.state?.presets[0]);
  const fileOf = (path: string) => path.split("\\").pop() ?? path;

  async function run(f: () => Promise<unknown>, ok: string) {
    msg = null;
    try {
      await f();
      msg = ok;
      await binds.load();
    } catch (e) {
      msg = String(e);
    }
  }
  const useFor = (x: Preset, i: number) => run(() => api.setActive(x.name, [i]), `${x.name} is now used for ${CONTEXTS[i]}. StartPreset backed up first.`);
  const useAll = (x: Preset) => run(() => api.setActive(x.name, [0, 1, 2, 3]), `${x.name} is now used everywhere. StartPreset backed up first.`);
</script>

<div class="cols">
  <section>
    <h3>Presets</h3>
    <div class="head"><span>Name</span><span>General</span><span>Ship</span><span>SRV</span><span>On foot</span><span></span></div>
    {#each binds.state?.presets ?? [] as x (x.path)}
      <div class="row" class:sel={p?.path === x.path}>
        <button class="name" onclick={() => (sel = x.path)}>{x.name}{#if x.source === "built-in"}<span class="lo"> · built-in</span>{/if}</button>
        {#each CONTEXTS as c, i (c)}
          <button class="cell" class:on={x.active_for.includes(c)} title="Use {x.name} for {c}" onclick={() => void useFor(x, i)}>{x.active_for.includes(c) ? "Active" : ""}</button>
        {/each}
        <span class="mark">{x.problems.length ? `! ${x.problems.length} issue${x.problems.length > 1 ? "s" : ""}` : ""}</span>
      </div>
    {:else}
      <p class="lo">{binds.state ? "No presets found. Run Elite once, or set the Bindings folder in Settings." : binds.error ?? "Loading…"}</p>
    {/each}
    <p class="lo">Click a cell to use that preset there (this writes StartPreset, like picking it in the game's Controls menu). Restart Elite, or pick the preset in Options → Controls, for it to take effect.</p>
    {#if msg}<p class="msg">{msg}</p>{/if}

    <h3 class="gap">Backups</h3>
    {#each binds.state?.backups ?? [] as b (b.path)}
      <div class="row b">
        <span class="name">{b.original}</span><span class="lo">{b.created}</span>
        <button class="act" onclick={() => void run(() => api.restore(b.path), `Restored ${b.original} (the current file was backed up first).`)}>Restore</button>
      </div>
    {:else}
      <p class="lo">No backups yet. astromech makes one before every change, in {binds.state?.bindings_dir ?? "the Bindings folder"}\astromech-backups.</p>
    {/each}
  </section>
  {#if p}
    <aside>
      <div class="lbl">{p.source === "built-in" ? "Built-in preset" : "Your preset"}</div>
      <h2>{p.name}</h2>
      <p>{p.active_for.length ? `Active for ${p.active_for.join(", ").toLowerCase()}.` : "Not active anywhere."}</p>
      <div class="k">Health</div>
      {#each p.problems as pr (pr)}<p class="warn">! {pr}</p>{:else}<p>No problems found.</p>{/each}
      <div class="k">Devices</div>
      <p>{p.devices.join(", ") || "Keyboard and mouse only"}</p>
      <div class="k">Files</div>
      {#each p.files as f (f.path)}<p class:lo={f.path !== p.path}>{f.file_name}{f.path === p.path ? " (used by the game)" : ""}<span class="lo"> · {f.modified}</span></p>{/each}
      <button class="act" onclick={() => void binds.show(p.path).then(() => (location.hash = "#/keybinds"))}>Open in all keybinds</button>
      <button class="act" onclick={() => void useAll(p)}>Use everywhere</button>
      <div class="k">Make a copy</div>
      <div class="copy">
        <input bind:value={copyName} placeholder="{p.name} copy" maxlength="40" />
        <button class="act" onclick={() => void run(() => api.copy(p.path, copyName || `${p.name} copy`), `Copied to ${copyName || `${p.name} copy`}.`)}>Copy</button>
      </div>
      <p class="lo">{fileOf(p.path)}</p>
    </aside>
  {/if}
</div>

<style>
  button { all: unset; cursor: pointer; box-sizing: border-box; }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) 380px; gap: 28px; padding-top: 18px; }
  h3 { margin: 0 0 10px; color: var(--accent); font-weight: 600; font-size: var(--fs-s); text-transform: uppercase; letter-spacing: var(--caps); }
  h3.gap { margin-top: 34px; }
  .head, .row { display: grid; grid-template-columns: 1fr repeat(4, 120px) 130px; gap: var(--gap); align-items: center; }
  .head { color: var(--text-lo); font-size: var(--fs-s); text-transform: uppercase; letter-spacing: var(--caps); padding: 0 0 6px; }
  .head span:not(:first-child) { padding-left: 12px; }
  .row { min-height: var(--row); margin-bottom: var(--gap); }
  .row.b { grid-template-columns: 1fr 220px 120px; }
  .name { height: var(--row); display: flex; align-items: center; }
  .row.sel .name { color: var(--accent); }
  .cell { height: var(--row); display: flex; align-items: center; padding: 0 12px; background: var(--cell-empty); color: var(--text-lo); }
  .cell.on { background: var(--cell); color: var(--text); }
  .cell:hover { outline: 1px solid var(--text-lo); outline-offset: -1px; }
  .mark { color: var(--accent); font-size: var(--fs-s); font-weight: 600; padding-left: 10px; }
  .lo { color: var(--text-lo); }
  .msg, .warn { color: var(--accent); }
  p { line-height: 1.45; }
  aside { padding: 4px 0 20px 28px; border-left: 2px solid var(--accent); }
  .lbl { color: var(--accent); font-size: var(--fs-s); text-transform: uppercase; letter-spacing: var(--caps); }
  h2 { margin: 6px 0 12px; color: var(--text-hi); font-weight: 500; font-size: var(--fs-xl); }
  aside p { margin: 0 0 8px; }
  .k { color: var(--text-lo); font-size: var(--fs-s); text-transform: uppercase; letter-spacing: var(--caps); margin: 18px 0 4px; }
  .act { display: block; margin: 8px 0; color: var(--accent); text-transform: uppercase; letter-spacing: var(--caps); font-weight: 600; font-size: var(--fs-s); }
  .act:hover { color: var(--text-hi); }
  .copy { display: flex; gap: 16px; align-items: center; }
  input { all: unset; flex: 1; height: var(--row); padding: 0 12px; background: var(--cell-opt); color: var(--text-hi); border-bottom: 2px solid var(--accent); }
  @media (max-width: 1200px) {
    .cols { grid-template-columns: minmax(0, 1fr) 260px; gap: 20px; }
    .head, .row { grid-template-columns: minmax(0, 1fr) repeat(4, 64px) 100px; }
    .row.b { grid-template-columns: minmax(0, 1fr) 120px 100px; }
    aside { padding-left: 18px; }
  }
</style>
