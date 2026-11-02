<script lang="ts">
  import { nestedComposition, nestedHistory, setNestedTimelineVideo } from '$lib/state/nestedComposition.svelte'
  import { timelineVideoReason } from '$lib/composition/timelineOrder'
  let { disabled = false }: { disabled?: boolean } = $props()
  const project = $derived(nestedComposition.document)
  const active = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  const canvas = $derived(project?.compositions[active]?.content.canvas)
  const presets = [
    { value: '24000/1001', label: '23,976', numerator: 24000, denominator: 1001 },
    { value: '24/1', label: '24', numerator: 24, denominator: 1 },
    { value: '25/1', label: '25', numerator: 25, denominator: 1 },
    { value: '30000/1001', label: '29,97', numerator: 30000, denominator: 1001 },
    { value: '30/1', label: '30', numerator: 30, denominator: 1 },
    { value: '50/1', label: '50', numerator: 50, denominator: 1 },
    { value: '60000/1001', label: '59,94', numerator: 60000, denominator: 1001 },
    { value: '60/1', label: '60', numerator: 60, denominator: 1 },
  ]
  function presetValue(n: number, d: number): string {
    if (!Number.isSafeInteger(n) || !Number.isSafeInteger(d) || n <= 0 || d <= 0) return 'custom'
    return presets.find(preset => BigInt(n) * BigInt(preset.denominator) === BigInt(preset.numerator) * BigInt(d))?.value ?? 'custom'
  }
  let customMode = $state(false)
  function choosePreset(value: string): void {
    const preset = presets.find(item => item.value === value)
    customMode = !preset
    if (preset) { numerator = preset.numerator; denominator = preset.denominator }
  }
  let width = $state(1920), height = $state(1080), numerator = $state(30), denominator = $state(1)
  const selectedPreset = $derived(customMode ? 'custom' : presetValue(numerator, denominator))
  $effect(() => {
    if (canvas) {
      width = canvas.width; height = canvas.height
      const rate = canvas.fpsRate ?? { numerator: canvas.fpsMilli, denominator: 1000 }
      numerator = rate.numerator; denominator = rate.denominator
      customMode = presetValue(rate.numerator, rate.denominator) === 'custom'
    }
  })
  const settings = $derived({ width, height, numerator, denominator })
  const reason = $derived(project ? timelineVideoReason(project, active, settings) : 'Нет открытого проекта')
</script>
{#if canvas}
  <fieldset class="composition-fieldset" disabled={disabled || nestedHistory.busy}>
    <legend>Кадр таймлайна</legend>
    <label>Ширина<input aria-label="Ширина кадра таймлайна" type="number" min="2" max="3840" step="2" value={width} oninput={(event) => { width = event.currentTarget.valueAsNumber }} /></label>
    <label>Высота<input aria-label="Высота кадра таймлайна" type="number" min="2" max="2160" step="2" value={height} oninput={(event) => { height = event.currentTarget.valueAsNumber }} /></label>
    <label>Частота кадров
      <select aria-label="Частота кадров таймлайна" value={selectedPreset} onchange={(event) => choosePreset(event.currentTarget.value)}>
        {#each presets as preset (preset.value)}<option value={preset.value}>{preset.label} FPS</option>{/each}
        <option value="custom">Своя частота</option>
      </select>
    </label>
    <details open={customMode}><summary>Точная частота кадров</summary>
    <label>FPS, числитель<input aria-label="Числитель FPS таймлайна" type="number" min="1" max="4294967295" step="1" value={numerator} oninput={(event) => { numerator = event.currentTarget.valueAsNumber; customMode = true }} /></label>
    <label>FPS, знаменатель<input aria-label="Знаменатель FPS таймлайна" type="number" min="1" max="4294967295" step="1" value={denominator} oninput={(event) => { denominator = event.currentTarget.valueAsNumber; customMode = true }} /></label>
    </details>
    <button class="btn ghost sm" type="button" disabled={disabled || nestedHistory.busy || reason !== null} title={reason ?? undefined} onclick={() => void setNestedTimelineVideo(active, settings)}>Применить настройки кадра</button>
    {#if reason}<p role="status">{reason}</p>{/if}
  </fieldset>
{/if}
