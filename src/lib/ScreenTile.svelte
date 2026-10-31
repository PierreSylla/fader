<script lang="ts">
  import type { Placed } from "./layout.ts";
  import { METHOD, type Screen } from "./screens.ts";

  let { screen, place }: { screen: Screen; place: Placed } = $props();

  const off = $derived(screen.value === null);
</script>

<div
  class="tile"
  class:off
  role="slider"
  aria-label={`${screen.id}, ${screen.label}`}
  aria-valuenow={screen.value ?? 0}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-orientation="vertical"
  aria-disabled={off}
  style:left={`${place.x}px`}
  style:top={`${place.y}px`}
  style:width={`${place.width}px`}
  style:height={`${place.height}px`}
>
  <div class="fill" style:height={`${screen.value ?? 0}%`}></div>
  <div class="label">
    <div class="name">
      <span class="output">{screen.id}</span>
      <span class="method">{METHOD[screen.backend.kind]}</span>
    </div>
    <span class="value">{screen.value ?? "—"}</span>
  </div>
</div>

<style>
  .tile {
    position: absolute;
    box-sizing: border-box;
    overflow: hidden;
    background: var(--track);
    border: var(--rule) solid var(--line);
  }

  .tile.off {
    background: var(--surface);
    border-style: dashed;
  }

  .fill {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    background: var(--fill);
  }

  .label {
    position: absolute;
    top: 0;
    left: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    box-sizing: border-box;
    max-width: 100%;
    padding: 4px 7px 5px;
    background: var(--surface);
    border-right: var(--rule) solid var(--line);
    border-bottom: var(--rule) solid var(--line);
  }

  .name {
    display: flex;
    flex-wrap: wrap;
    column-gap: var(--sp-3);
    align-items: baseline;
  }

  .output {
    font-weight: var(--fw-strong);
    white-space: nowrap;
  }

  .method {
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }

  .value {
    font-size: var(--fs-value);
    font-variant-numeric: var(--num);
    line-height: 1.1;
  }
</style>
