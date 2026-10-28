<script lang="ts">
  import { css } from "../lib/dom";
  let { min, max, step, value, width, onChange }: { min: number; max: number; step: number; value: number; width: number; onChange: (value: number) => void } = $props();
  let pct = $derived(max === min ? 0 : ((value - min) / (max - min)) * 100);
  const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
  function commitFromClientX(element: HTMLElement, clientX: number) {
    const rect = element.getBoundingClientRect();
    const ratio = clamp((clientX - rect.left) / rect.width, 0, 1);
    const raw = min + ratio * (max - min);
    onChange(clamp(Math.round(raw / step) * step, min, max));
  }
  function release(event: PointerEvent & { currentTarget: EventTarget & HTMLSpanElement }) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }
</script>

<span class="material-slider" role="group" aria-label="Slider" style={css({ width, "--slider-p": `${pct}%` })}
  onpointerdown={(event) => { event.currentTarget.setPointerCapture(event.pointerId); commitFromClientX(event.currentTarget, event.clientX); }}
  onpointermove={(event) => { if (event.currentTarget.hasPointerCapture(event.pointerId)) commitFromClientX(event.currentTarget, event.clientX); }}
  onpointerup={release} onpointercancel={release}>
  <span class="material-slider-track inactive left"></span>
  <span class="material-slider-track active"></span>
  <span class="material-slider-track inactive right"></span>
  <span class="material-slider-thumb"></span>
  <input type="range" {min} {max} {step} {value} oninput={(event) => onChange(Number(event.currentTarget.value))} aria-label="Slider" tabindex="0" />
</span>
