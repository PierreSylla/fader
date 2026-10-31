<script lang="ts">
  import { t } from "./i18n.ts";
  import { valueFromBottom, valueFromKey, valueFromWheel } from "./input.ts";
  import type { Placed } from "./layout.ts";
  import { METHOD, type Screen } from "./screens.ts";

  let {
    screen,
    place,
    onchange,
    onactive,
  }: {
    screen: Screen;
    place: Placed;
    onchange: (id: string, value: number) => void;
    onactive: (id: string, active: boolean) => void;
  } = $props();

  let dragging = $state(false);
  let hover = $state<number | null>(null);

  const off = $derived(screen.value === null);
  const value = $derived(screen.value ?? 0);
  const busy = $derived(
    screen.status === "busy" && screen.applied !== null && !dragging,
  );
  const shown = $derived(busy ? (screen.applied ?? 0) : value);
  const method = $derived(
    METHOD[screen.backend.kind] + (busy ? ` · ${t("sending")}` : ""),
  );
  const reason = $derived(
    screen.status === "permission" ? t("I²C access denied") : screen.reason,
  );

  const onpointerdown = (event: PointerEvent) => {
    if (off) return;
    const tile = event.currentTarget as HTMLElement;
    tile.setPointerCapture(event.pointerId);
    dragging = true;
    onchange(
      screen.id,
      valueFromBottom(tile.getBoundingClientRect(), event.clientY),
    );
  };

  const onpointermove = (event: PointerEvent) => {
    if (off) return;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    hover = valueFromBottom(rect, event.clientY);
    if (dragging) onchange(screen.id, hover);
  };

  const onkeydown = (event: KeyboardEvent) => {
    if (off) return;
    const next = valueFromKey(event.key, value);
    if (next === null) return;
    event.preventDefault();
    onchange(screen.id, next);
  };

  const onwheel = (event: WheelEvent) => {
    if (off) return;
    event.preventDefault();
    onchange(screen.id, valueFromWheel(event.deltaY, event.shiftKey, value));
  };
</script>

<div
  class="tile"
  class:off
  class:dragging
  role="slider"
  tabindex={off ? -1 : 0}
  aria-label={`${screen.id}, ${screen.label}`}
  aria-valuenow={value}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-orientation="vertical"
  aria-disabled={off}
  aria-busy={busy}
  style:left={`${place.x}px`}
  style:top={`${place.y}px`}
  style:width={`${place.width}px`}
  style:height={`${place.height}px`}
  {onpointerdown}
  {onpointermove}
  onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)}
  onpointerenter={() => onactive(screen.id, true)}
  onpointerleave={() => {
    hover = null;
    onactive(screen.id, false);
  }}
  onfocus={() => onactive(screen.id, true)}
  onblur={() => onactive(screen.id, false)}
  {onkeydown}
  {onwheel}
>
  {#if !off}
    <div class="fill" style:height={`${shown}%`}></div>
  {/if}
  {#if busy}
    <div class="target" style:bottom={`${value}%`}></div>
  {/if}
  {#if hover !== null}
    <div class="marker" style:bottom={`${hover}%`}>
      <span class:below={hover > 80}>{hover}</span>
    </div>
  {/if}
  <div class="label">
    <div class="name">
      <span class="output">{screen.id}</span>
      <span class="method">{method}</span>
    </div>
    <div class="reading">
      <span class="value" class:muted={busy}>{off ? "—" : shown}</span>
      {#if busy}
        <span class="goal">→ {value}</span>
      {/if}
    </div>
  </div>
  {#if off}
    <div class="reason">
      <span>{reason}</span>
      <code>fader doctor</code>
    </div>
  {/if}
</div>

<style>
  .tile {
    position: absolute;
    box-sizing: border-box;
    overflow: hidden;
    cursor: ns-resize;
    touch-action: none;
    background: var(--track);
    border: var(--rule) solid var(--line);
    outline: none;
  }

  .tile:focus {
    outline: var(--focus-ring) solid var(--focus);
    outline-offset: var(--focus-offset);
  }

  .tile.dragging {
    border-color: var(--text);
  }

  .tile.off {
    cursor: not-allowed;
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

  .tile:hover .fill,
  .tile:focus .fill,
  .tile.dragging .fill {
    background: var(--fill-active);
  }

  .target {
    position: absolute;
    right: 0;
    left: 0;
    border-top: var(--rule) dashed var(--text);
  }

  .marker {
    position: absolute;
    right: 0;
    left: 0;
    border-top: var(--rule) solid var(--text);
  }

  .marker span {
    position: absolute;
    right: 3px;
    bottom: 2px;
    padding: 0 3px;
    color: var(--text);
    font-size: var(--fs-xs);
    font-variant-numeric: var(--num);
    background: var(--surface);
  }

  .marker span.below {
    top: 2px;
    bottom: auto;
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

  .reading {
    display: flex;
    gap: 5px;
    align-items: baseline;
    white-space: nowrap;
  }

  .value {
    font-size: var(--fs-value);
    font-variant-numeric: var(--num);
    line-height: 1.1;
  }

  .value.muted {
    color: var(--text-muted);
  }

  .goal {
    font-variant-numeric: var(--num);
  }

  .reason {
    position: absolute;
    right: var(--sp-3);
    bottom: var(--sp-3);
    left: var(--sp-3);
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }

  .reason span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  code {
    color: var(--text);
    font-family: var(--font-mono);
  }
</style>
