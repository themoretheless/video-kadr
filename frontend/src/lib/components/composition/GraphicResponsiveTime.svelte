<script lang="ts">
  import type { GraphicResponsiveTime } from '$lib/composition/types'
  let { value, duration, localUs, onchange }: { value?: GraphicResponsiveTime; duration: number; localUs: number; onchange: (value: GraphicResponsiveTime | undefined) => void } = $props()
  const fields = [['intro', 'Вступление'], ['outro', 'Финал'], ['designIn', 'Начало исходного времени'], ['designDuration', 'Исходная длительность']] as const
  let error = $state('')
  function edit(key: keyof GraphicResponsiveTime, raw: string) {
    const parsed = Number(raw)
    if (!raw.trim() || !Number.isSafeInteger(parsed)) {error = 'Укажите целое число микросекунд';return}
    const next = {...(value ?? {intro:0,outro:0,designIn:0,designDuration:duration}),[key]:parsed}
    if (next.intro < 0 || next.outro < 0 || next.designDuration <= 0) {error = 'Интервалы должны быть неотрицательными, исходная длительность — положительной';return}
    error = ''; onchange(next)
  }
</script>
<details>
  <summary>Защищённые интервалы</summary>
  <p>Вступление и финал сохраняют скорость. Между ними время растягивается. Значения в микросекундах.</p>
  {#each fields as [key, label]}
    <label>{label}<input type="number" step="1" aria-label={`Графика ${key}`} value={value?.[key] ?? (key === 'designDuration' ? duration : 0)} onchange={event => edit(key,event.currentTarget.value)} /></label>
  {/each}
  <div class="actions">
    <button class="btn ghost sm" type="button" onclick={() => onchange({intro:Math.max(0,Math.min(duration,localUs)),outro:value?.outro ?? 0,designIn:value?.designIn ?? 0,designDuration:value?.designDuration ?? duration})}>Вступление до курсора</button>
    <button class="btn ghost sm" type="button" onclick={() => onchange({intro:value?.intro ?? 0,outro:Math.max(0,duration-Math.max(0,localUs)),designIn:value?.designIn ?? 0,designDuration:value?.designDuration ?? duration})}>Финал от курсора</button>
    <button class="btn ghost sm" type="button" disabled={!value} onclick={() => onchange(undefined)}>Удалить защиту</button>
  </div>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>
  label { display:flex; align-items:center; justify-content:space-between; gap:.4rem; }
  input { width:8rem; }
  p { font-size:.8rem; }
  .actions { display:flex; gap:.3rem; flex-wrap:wrap; }
</style>
