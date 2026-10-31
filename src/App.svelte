<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { t } from "./lib/i18n.ts";
  import { computeLayout } from "./lib/layout.ts";
  import NativeRow from "./lib/NativeRow.svelte";
  import ScreenTile from "./lib/ScreenTile.svelte";
  import {
    hidePanel,
    listScreens,
    METHOD_LONG,
    panelIsOpen,
    resizePanel,
    setBrightness,
    type Screen,
  } from "./lib/screens.ts";

  let open = $state(false);
  let ready = $state(false);
  let screens = $state<Screen[]>([]);
  let activeId = $state<string | null>(null);
  let hoveringPanel = $state(false);
  let changes = 0;

  const ordered = $derived(
    [...screens].sort((a, b) => a.geometry.x - b.geometry.x),
  );
  const native = $derived(ordered.length === 1);
  const layout = $derived(
    computeLayout(ordered.map((s) => ({ id: s.id, ...s.geometry }))),
  );
  const active = $derived(
    ordered.find((s) => s.id === activeId) ??
      (native && ordered[0].value === null ? ordered[0] : undefined),
  );
  const denied = $derived(ordered.some((s) => s.status === "permission"));
  const failed = $derived(
    ordered.find((s) => s.value !== null && s.reason !== null),
  );
  const help = $derived(
    native
      ? t("Drag on the bar · wheel · ↑↓ ±1 · PgUp/PgDn ±10")
      : t("Drag vertically on a screen · wheel · ↑↓ ±1 · PgUp/PgDn ±10"),
  );
  const context = $derived.by(() => {
    if (!active) return hoveringPanel && ordered.length > 0 ? help : "";
    const parts = [
      active.id,
      active.label,
      t(METHOD_LONG[active.backend.kind]),
    ];
    if (active.reason) parts.push(active.reason);
    if (active.status === "busy") parts.push(t("applying"));
    return parts.join(" — ");
  });

  const load = async () => {
    const started = changes;
    const list = await listScreens();
    // A reply older than the latest change would make the tile jump back.
    if (started !== changes) return;
    ready = list.ready;
    screens = list.screens;
  };

  const change = (id: string, value: number) => {
    const screen = screens.find((s) => s.id === id);
    if (!screen || screen.value === value) return;
    changes += 1;
    screen.value = value;
    setBrightness(id, value).catch(load);
  };

  const setActive = (id: string, isActive: boolean) => {
    if (isActive) activeId = id;
    else if (activeId === id) activeId = null;
  };

  load();
  listen("screens-changed", load);
  listen<boolean>("panel-visibility", (event) => {
    open = event.payload;
    if (open) load();
    else if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  }).then(async () => {
    // The panel may have been opened while this page was loading.
    open = await panelIsOpen();
  });

  const reportSize = (panel: HTMLElement) => {
    const observer = new ResizeObserver(() => {
      const { width, height } = panel.getBoundingClientRect();
      resizePanel(Math.round(width), Math.round(height));
    });
    observer.observe(panel);
    return () => observer.disconnect();
  };

  const onKeydown = (event: KeyboardEvent) => {
    if (open && event.key === "Escape") hidePanel();
  };
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="panel"
  class:open
  class:native
  role="group"
  aria-label={t("Brightness")}
  onpointerenter={() => (hoveringPanel = true)}
  onpointerleave={() => (hoveringPanel = false)}
  {@attach reportSize}
>
  <header><span>{t("Brightness")}</span></header>

  {#if denied}
    <div class="banner">
      <span class="dot"></span>
      <span class="text"
        >{t("I²C access denied: DDC screens cannot be adjusted.")}</span
      >
      <code>fader doctor</code>
    </div>
  {:else if failed}
    <div class="banner">
      <span class="dot"></span>
      <span class="text">{failed.id} — {failed.reason}</span>
      <code>fader doctor</code>
    </div>
  {/if}

  {#if ordered.length === 0}
    <div class="message">
      {#if ready}
        <span>{t("No screen detected.")}</span>
        <span class="hint">{t("Run:")} <code>fader doctor</code></span>
      {:else}
        <span class="hint">{t("Looking for screens")}</span>
      {/if}
    </div>
  {:else if native}
    <NativeRow screen={ordered[0]} onchange={change} onactive={setActive} />
  {:else}
    <div class="content">
      <div
        class="screens"
        style:width={`${layout.width}px`}
        style:height={`${layout.height}px`}
      >
        {#each layout.screens as place, i (place.id)}
          <ScreenTile
            screen={ordered[i]}
            {place}
            onchange={change}
            onactive={setActive}
          />
        {/each}
      </div>
    </div>
  {/if}

  <div class="context" class:current={active}>{context || "\u00a0"}</div>
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

  .panel.native {
    width: 360px;
  }

  header {
    display: flex;
    align-items: center;
    height: var(--header-h);
    padding: 0 var(--sp-5);
    font-weight: var(--fw-strong);
    border-bottom: var(--rule) solid var(--line);
  }

  .banner {
    display: flex;
    gap: var(--sp-3);
    align-items: baseline;
    padding: var(--sp-3) var(--sp-5);
    background: var(--surface);
    border-bottom: var(--rule) solid var(--line);
  }

  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    background: var(--warn);
    transform: translateY(-1px);
  }

  .banner .text {
    flex: 1;
    min-width: 0;
  }

  .content {
    display: flex;
    justify-content: center;
    padding: var(--sp-6) var(--sp-6) var(--sp-4);
  }

  .screens {
    position: relative;
  }

  .message {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    padding: var(--sp-6) var(--sp-5);
  }

  .hint {
    color: var(--text-muted);
  }

  code {
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    white-space: nowrap;
  }

  .context {
    padding: var(--sp-3) var(--sp-5);
    color: var(--text-muted);
    border-top: var(--rule) solid var(--line);
  }

  .context.current {
    overflow: hidden;
    color: var(--text);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
