<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { t } from "./lib/i18n.ts";
  import { computeLayout } from "./lib/layout.ts";
  import ScreenTile from "./lib/ScreenTile.svelte";
  import {
    hidePanel,
    listScreens,
    panelIsOpen,
    resizePanel,
    type Screen,
  } from "./lib/screens.ts";

  let open = $state(false);
  let screens = $state<Screen[]>([]);

  const ordered = $derived(
    [...screens].sort((a, b) => a.geometry.x - b.geometry.x),
  );
  const layout = $derived(
    computeLayout(ordered.map((s) => ({ id: s.id, ...s.geometry }))),
  );

  const load = async () => {
    screens = await listScreens();
  };

  load();
  listen("screens-changed", load);
  listen<boolean>("panel-visibility", (event) => {
    open = event.payload;
    if (open) load();
  }).then(async () => {
    // The panel may have been opened while this page was loading.
    open = await panelIsOpen();
  });

  const reportHeight = (panel: HTMLElement) => {
    const observer = new ResizeObserver(() => {
      resizePanel(Math.ceil(panel.getBoundingClientRect().height));
    });
    observer.observe(panel);
    return () => observer.disconnect();
  };

  const onKeydown = (event: KeyboardEvent) => {
    if (open && event.key === "Escape") hidePanel();
  };
</script>

<svelte:window onkeydown={onKeydown} />

<div class="panel" class:open {@attach reportHeight}>
  <header><span>{t("Brightness")}</span></header>
  <div class="content">
    <div
      class="screens"
      style:width={`${layout.width}px`}
      style:height={`${layout.height}px`}
    >
      {#each layout.screens as place, i (place.id)}
        <ScreenTile screen={ordered[i]} {place} />
      {/each}
    </div>
  </div>
  <div class="context">&nbsp;</div>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    background: transparent;
  }

  .panel {
    box-sizing: border-box;
    width: var(--panel-max-w);
    color: var(--text);
    font-family: var(--font-ui);
    font-size: var(--fs-sm);
    line-height: var(--lh);
    background: var(--bg);
    border: var(--rule) solid var(--line);
    border-radius: var(--radius-panel);
    visibility: hidden;
  }

  .panel.open {
    visibility: visible;
  }

  header {
    display: flex;
    align-items: center;
    height: var(--header-h);
    padding: 0 var(--sp-5);
    font-weight: var(--fw-strong);
    border-bottom: var(--rule) solid var(--line);
  }

  .content {
    display: flex;
    justify-content: center;
    padding: var(--sp-6) var(--sp-6) var(--sp-4);
  }

  .screens {
    position: relative;
  }

  .context {
    padding: var(--sp-3) var(--sp-5);
    border-top: var(--rule) solid var(--line);
  }
</style>
