<script lang="ts">
  // The window frame (docs/V1-DESIGN.md §2.2, §4): title, plain text tabs for the three areas,
  // one orange rule, the area's pages as sub-tabs, the page, and a one-line core state. Opens on
  // My controller. Onboarding replaces the frame on first launch (until finished or skipped) and
  // at #/setup or #/setup/<step>. Ctrl+K opens the command palette from any page.
  import { groups, groupModules, findModule, modules } from "./lib/modules";
  import { collect } from "./lib/palette";
  import Palette from "./lib/ui/Palette.svelte";
  import { coreStatus, headerState, onRoute, type CoreStatus } from "./lib/core";
  import Toasts from "./lib/ui/Toasts.svelte";
  import Onboarding from "./lib/onboarding/Onboarding.svelte";
  import { openHelp } from "./lib/help";

  const DEFAULT = "controller";
  // Pages that were merged into another keep working as links.
  const MOVED: Record<string, string> = { activities: "controller" };
  const resolve = (r: string) => MOVED[r] ?? (r || DEFAULT);
  const fromHash = () => resolve(location.hash.replace(/^#\//, ""));
  const start = fromHash();
  let route = $state(start);
  if (location.hash && location.hash !== `#/${start}`) location.hash = `#/${start}`;
  // A page may route within itself ("engineering/7" opens that ship): the module is the first part.
  const current = $derived(findModule(route.split("/")[0] ?? "") ?? findModule(DEFAULT));
  const pages = $derived(current ? groupModules(current.group) : []);
  let status = $state<CoreStatus | null>(null);
  const setup = $derived(status?.setup_needed ?? []);
  const onboarding = $derived(route === "setup" || route.startsWith("setup/") || status?.onboarded === false);

  function go(id: string) {
    route = resolve(id);
    location.hash = `#/${route}`;
  }

  $effect(() => {
    const hash = () => (route = fromHash());
    window.addEventListener("hashchange", hash);
    return () => window.removeEventListener("hashchange", hash);
  });

  const groupTitle = (id: string) => groups.find((g) => g.id === id)?.title ?? id;
  const paletteItems = () => collect(modules, groupTitle);

  async function finishSetup() {
    status = await coreStatus();
    go(DEFAULT);
  }

  $effect(() => {
    onRoute((r) => go(r));
    const tick = async () => (status = await coreStatus());
    void tick();
    const t = setInterval(() => void tick(), 2000);
    return () => clearInterval(t);
  });
</script>

{#if onboarding}
  <Onboarding start={route.split("/")[1]} onfinish={finishSetup} />
{:else}
<div class="frame">
  <header>
    <span class="app">ASTROMECH</span>
    <nav>
      {#each groups as g (g.id)}
        {@const first = groupModules(g.id)[0]}
        {#if first}
          <button class:on={g.id === current?.group} onclick={() => go(first.id)}>{g.title}</button>
        {/if}
      {/each}
    </nav>
    {#if setup.length && current?.id !== "settings"}
      <button class="setup" title={setup.join(", ")} onclick={() => go("setup/health")}>Finish setup</button>
    {/if}
    <span class="right">
      <span class="state">{headerState(status)}</span>
      {#if current}
        <button class="help" title="Open this page's guide in your browser" onclick={() => void openHelp(current.help)}>Help</button>
      {/if}
    </span>
  </header>
  {#if pages.length > 1}
    <nav class="sub">
      {#each pages as m (m.id)}
        <button class:on={m.id === current?.id} title={m.summary} onclick={() => go(m.id)}>{m.title}</button>
      {/each}
    </nav>
  {/if}
  <main>
    {#if current}
      {#await current.load() then view}
        <view.default />
      {/await}
    {/if}
  </main>
  <Toasts {status} />
</div>
<Palette load={paletteItems} onpick={go} />
{/if}

<style>
  .frame { height: 100vh; display: grid; grid-template-rows: auto auto 1fr; }
  header { display: flex; align-items: baseline; gap: 40px; margin: 0 40px; padding: 26px 0 12px; border-bottom: 1px solid var(--accent); }
  .app { font-weight: 600; font-size: var(--fs-l); letter-spacing: 0.12em; color: var(--text-hi); }
  nav { display: flex; gap: 28px; flex: 1; flex-wrap: wrap; }
  button { all: unset; cursor: pointer; color: var(--text-lo); text-transform: uppercase; letter-spacing: var(--caps); font-weight: 500; }
  button:hover { color: var(--text); }
  button.on { color: var(--accent); }
  nav.sub { flex: none; gap: 24px; margin: 0 40px; padding: 12px 0 0; }
  nav.sub button { font-size: var(--fs-s); font-weight: 600; padding-bottom: 4px; border-bottom: 2px solid transparent; }
  nav.sub button.on { color: var(--text-hi); border-bottom-color: var(--accent); }
  .setup { color: var(--accent); font-size: var(--fs-s); font-weight: 600; }
  .setup:hover { color: var(--text-hi); }
  .help { font-size: var(--fs-s); font-weight: 600; text-decoration: underline; text-underline-offset: 3px; }
  .right { display: flex; align-items: baseline; gap: 16px; }
  .state { color: var(--text-lo); font-size: var(--fs-s); text-transform: uppercase; letter-spacing: var(--caps); white-space: nowrap; }
  main { min-height: 0; overflow: auto; padding: 0 40px; scrollbar-gutter: stable; }
  @media (max-width: 1100px) {
    header { gap: 24px; margin: 0 24px; }
    nav { gap: 18px; }
    nav.sub { margin: 0 24px; }
    main { padding: 0 24px; }
  }
</style>
