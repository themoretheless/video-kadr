<script lang="ts">
  import type { ColorWheelAdjustment, HslBandAdjustment, SelectiveHsl } from '$lib/types.js'
  import { state as appState } from '$lib/state/store.svelte.js'

  type HslBandName = keyof SelectiveHsl
  type WheelName = 'shadows' | 'midtones' | 'highlights'

  const bands: readonly { id: HslBandName; label: string; color: string }[] = [
    { id: 'red', label: 'Красные', color: '#ef4444' },
    { id: 'yellow', label: 'Жёлтые', color: '#eab308' },
    { id: 'green', label: 'Зелёные', color: '#22c55e' },
    { id: 'cyan', label: 'Голубые', color: '#06b6d4' },
    { id: 'blue', label: 'Синие', color: '#3b82f6' },
    { id: 'magenta', label: 'Пурпурные', color: '#d946ef' },
  ]
  const wheels: readonly { id: WheelName; label: string }[] = [
    { id: 'shadows', label: 'Тени' },
    { id: 'midtones', label: 'Средние тона' },
    { id: 'highlights', label: 'Света' },
  ]

  let selectedBand = $state<HslBandName>('red')
  const activeBand = $derived(appState.edit.hsl[selectedBand])
  const hslCapability = $derived(appState.capabilities?.filters?.find((option) => option.id === 'selective-hsl'))
  const wheelsCapability = $derived(appState.capabilities?.filters?.find((option) => option.id === 'color-wheels'))
  const hslUnavailable = $derived(capabilityReason(hslCapability, 'huesaturation'))
  const wheelsUnavailable = $derived(capabilityReason(wheelsCapability, 'colorbalance'))

  function capabilityReason(
    capability: { available: boolean; reason?: string } | undefined,
    filter: string,
  ): string {
    if (!appState.capabilities) return ''
    if (!capability) return `Нужен обновлённый сервер с FFmpeg filter ${filter}`
    return capability.available ? '' : capability.reason || `FFmpeg filter ${filter} недоступен`
  }

  function setBandValue(field: keyof HslBandAdjustment, event: Event): void {
    appState.edit.hsl[selectedBand][field] = Number((event.currentTarget as HTMLInputElement).value)
  }

  function resetBand(): void {
    appState.edit.hsl[selectedBand] = { hue: 0, saturation: 0, lightness: 0 }
  }

  function setWheelValue(wheel: WheelName, channel: keyof ColorWheelAdjustment, event: Event): void {
    appState.edit.colorWheels[wheel][channel] = Number((event.currentTarget as HTMLInputElement).value)
  }

  function resetWheels(): void {
    appState.edit.colorWheels = {
      shadows: { red: 0, green: 0, blue: 0 },
      midtones: { red: 0, green: 0, blue: 0 },
      highlights: { red: 0, green: 0, blue: 0 },
      preserveLuminosity: true,
    }
  }

  function wheelSwatch(value: ColorWheelAdjustment): string {
    const channel = (amount: number): number => Math.round(128 + amount * 127)
    return `rgb(${channel(value.red)} ${channel(value.green)} ${channel(value.blue)})`
  }
</script>

<div class="manual-color-tools">
  <details class="color-tool" class:disabled={Boolean(hslUnavailable)}>
    <summary>
      <span><strong>HSL по цветам</strong><small>Оттенок, насыщенность и светлота шести диапазонов</small></span>
      <span aria-hidden="true">⌄</span>
    </summary>
    {#if hslUnavailable}
      <p class="unavailable" role="status">{hslUnavailable}</p>
    {:else}
      <div class="band-tabs" role="tablist" aria-label="Цветовой диапазон HSL">
        {#each bands as band (band.id)}
          <button
            type="button"
            role="tab"
            aria-selected={selectedBand === band.id}
            class:active={selectedBand === band.id}
            onclick={() => { selectedBand = band.id }}
          ><span style:background={band.color}></span>{band.label}</button>
        {/each}
      </div>
      <div class="slider-stack" aria-label={`HSL: ${bands.find((band) => band.id === selectedBand)?.label ?? selectedBand}`}>
        <label>Оттенок <output>{Math.round(activeBand.hue)}°</output><input type="range" min="-180" max="180" step="1" value={activeBand.hue} oninput={(event) => setBandValue('hue', event)} /></label>
        <label>Насыщенность <output>{Math.round(activeBand.saturation * 100)}%</output><input type="range" min="-1" max="1" step="0.01" value={activeBand.saturation} oninput={(event) => setBandValue('saturation', event)} /></label>
        <label>Светлота <output>{Math.round(activeBand.lightness * 100)}%</output><input type="range" min="-1" max="1" step="0.01" value={activeBand.lightness} oninput={(event) => setBandValue('lightness', event)} /></label>
        <button class="reset" type="button" onclick={resetBand}>Сбросить диапазон</button>
      </div>
    {/if}
  </details>

  <details class="color-tool" class:disabled={Boolean(wheelsUnavailable)}>
    <summary>
      <span><strong>Цветовые колёса</strong><small>Баланс RGB отдельно для теней, средних тонов и светов</small></span>
      <span aria-hidden="true">⌄</span>
    </summary>
    {#if wheelsUnavailable}
      <p class="unavailable" role="status">{wheelsUnavailable}</p>
    {:else}
      <div class="wheel-grid">
        {#each wheels as wheel (wheel.id)}
          <fieldset>
            <legend><span class="wheel-swatch" style:background={wheelSwatch(appState.edit.colorWheels[wheel.id])}></span>{wheel.label}</legend>
            {#each ['red', 'green', 'blue'] as channel (channel)}
              <label class={`channel ${channel}`}>
                <span>{channel === 'red' ? 'R' : channel === 'green' ? 'G' : 'B'}</span>
                <input
                  aria-label={`${wheel.label}: ${channel}`}
                  type="range"
                  min="-1"
                  max="1"
                  step="0.01"
                  value={appState.edit.colorWheels[wheel.id][channel as keyof ColorWheelAdjustment]}
                  oninput={(event) => setWheelValue(wheel.id, channel as keyof ColorWheelAdjustment, event)}
                />
                <output>{appState.edit.colorWheels[wheel.id][channel as keyof ColorWheelAdjustment].toFixed(2)}</output>
              </label>
            {/each}
          </fieldset>
        {/each}
      </div>
      <div class="wheel-footer">
        <label><input type="checkbox" bind:checked={appState.edit.colorWheels.preserveLuminosity} /> Сохранять яркость</label>
        <button class="reset" type="button" onclick={resetWheels}>Сбросить колёса</button>
      </div>
    {/if}
  </details>
  <p class="export-note">HSL и цветовые колёса рассчитываются локально при экспорте.</p>
</div>

<style>
  .manual-color-tools { display: grid; gap: .65rem; }
  .color-tool { border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); overflow: clip; }
  .color-tool.disabled { opacity: .72; }
  summary { display: flex; align-items: center; justify-content: space-between; gap: .75rem; padding: .8rem; cursor: pointer; }
  summary > span:first-child { display: grid; gap: .15rem; }
  summary small, .export-note, .unavailable { color: var(--muted); font-size: .75rem; }
  .band-tabs { display: grid; grid-template-columns: repeat(3, 1fr); gap: .35rem; padding: 0 .8rem .75rem; }
  .band-tabs button { display: flex; align-items: center; gap: .35rem; min-width: 0; padding: .4rem .45rem; border: 1px solid var(--border); border-radius: 7px; background: transparent; color: inherit; font-size: .72rem; cursor: pointer; }
  .band-tabs button.active { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .band-tabs button span { width: .65rem; height: .65rem; flex: 0 0 auto; border-radius: 50%; }
  .slider-stack { display: grid; gap: .65rem; padding: 0 .8rem .85rem; }
  .slider-stack label { display: grid; grid-template-columns: 1fr auto; gap: .3rem .7rem; font-size: .76rem; }
  .slider-stack input { grid-column: 1 / -1; width: 100%; }
  output { font-variant-numeric: tabular-nums; color: var(--muted); }
  .wheel-grid { display: grid; gap: .75rem; padding: 0 .8rem .8rem; }
  fieldset { display: grid; gap: .4rem; min-width: 0; margin: 0; padding: .65rem; border: 1px solid var(--border); border-radius: 8px; }
  legend { display: flex; align-items: center; gap: .45rem; padding: 0 .3rem; font-size: .78rem; font-weight: 700; }
  .wheel-swatch { width: 1rem; height: 1rem; border: 1px solid color-mix(in srgb, currentColor 25%, transparent); border-radius: 50%; }
  .channel { display: grid; grid-template-columns: 1rem 1fr 3rem; gap: .45rem; align-items: center; font-size: .7rem; }
  .channel.red span { color: #ef4444; } .channel.green span { color: #22c55e; } .channel.blue span { color: #3b82f6; }
  .wheel-footer { display: flex; align-items: center; justify-content: space-between; gap: .7rem; padding: 0 .8rem .85rem; font-size: .75rem; }
  .reset { justify-self: end; padding: .3rem .55rem; border: 1px solid var(--border); border-radius: 6px; background: transparent; color: inherit; cursor: pointer; font-size: .7rem; }
  .unavailable, .export-note { margin: 0; padding: 0 .8rem .8rem; }
</style>
