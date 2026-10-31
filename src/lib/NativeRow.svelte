<script lang="ts">
  import { valueFromKey, valueFromLeft, valueFromWheel } from "./input.ts";
  import { METHOD, type Screen } from "./screens.ts";

  let {
    screen,
    onchange,
    onactive,
  }: {
    screen: Screen;
    onchange: (id: string, value: number) => void;
    onactive: (id: string, active: boolean) => void;
  } = $props();

  let dragging = $state(false);

  const off = $derived(screen.value === null);
  const value = $derived(screen.value ?? 0);

  const fromPointer = (event: PointerEvent) => {
    const bar = event.currentTarget as HTMLElement;
    onchange(
      screen.id,
      valueFromLeft(bar.getBoundingClientRect(), event.clientX),
    );
  };

  const onkeydown = (event: KeyboardEvent) => {
    if (off) return;
    const next = valueFromKey(event.key, value);
    if (next === null) return;
    event.preventDefault();
    onchange(screen.id, next);
  };
</script>

<div class="row">
  <div class="name">
    <span class="output">{screen.id}</span>
    <span class="model">{screen.label}</span>
    <span class="method">{METHOD[screen.backend.kind]}</span>
  </div>
  <span class="value">{off ? "—" : value}</span>
  <div
    class="bar"
    class:off
    role="slider"
    tabindex={off ? -1 : 0}
    aria-label={`${screen.id}, ${screen.label}`}
    aria-valuenow={value}
    aria-valuemin="0"
    aria-valuemax="100"
    aria-disabled={off}
    aria-busy={screen.status === "busy"}
    onpointerdown={(event) => {
      if (off) return;
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
      dragging = true;
      fromPointer(event);
    }}
    onpointermove={(event) => {
      if (dragging) fromPointer(event);
    }}
    onpointerup={() => (dragging = false)}
    onpointercancel={() => (dragging = false)}
    onpointerenter={() => onactive(screen.id, true)}
    onpointerleave={() => onactive(screen.id, false)}
    onfocus={() => onactive(screen.id, true)}
    onblur={() => onactive(screen.id, false)}
    {onkeydown}
    onwheel={(event) => {
      if (off) return;
      event.preventDefault();
      onchange(screen.id, valueFromWheel(event.deltaY, event.shiftKey, value));
    }}
  >
    <div class="track"></div>
    {#if !off}
      <div class="fill" style:width={`${value}%`}></div>
      <div class="grip" style:left={`${value}%`}></div>
    {/if}
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--sp-1) var(--sp-5);
    padding: var(--sp-4) var(--sp-5) var(--sp-3);
  }

  .name {
    display: flex;
    gap: var(--sp-3);
    align-items: baseline;
    min-width: 0;
  }

  .output {
    font-weight: var(--fw-strong);
    white-space: nowrap;
  }

  .model {
    flex: 1;
    overflow: hidden;
    color: var(--text-muted);
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .method {
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }

  .value {
    grid-row: span 2;
    align-self: center;
    min-width: 3ch;
    font-size: var(--fs-value);
    font-variant-numeric: var(--num);
    line-height: 1;
    text-align: right;
  }

  .bar {
    position: relative;
    height: var(--native-hit-h);
    cursor: ew-resize;
    touch-action: none;
    outline: none;
  }

  .bar.off {
    cursor: not-allowed;
  }

  .track,
  .fill {
    position: absolute;
    top: 50%;
    left: 0;
    height: var(--native-track-h);
    transform: translateY(-50%);
  }

  .track {
    right: 0;
    background: var(--track);
  }

  .fill {
    background: var(--fill);
  }

  .bar:focus .track,
  .bar:focus .fill {
    height: var(--native-track-h-focus);
  }

  .bar:focus .fill,
  .bar:hover .fill {
    background: var(--fill-active);
  }

  .grip {
    position: absolute;
    top: 3px;
    width: 4px;
    height: 14px;
    margin-left: -2px;
    background: var(--text);
  }

  .bar:focus .grip {
    outline: var(--focus-ring-native) solid var(--focus);
    outline-offset: var(--focus-offset);
  }
</style>
