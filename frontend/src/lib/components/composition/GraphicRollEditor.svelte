<script lang="ts">
  import type { GraphicRoll } from '$lib/composition/types'
  let {value,onchange}:{value?:GraphicRoll;onchange:(value:GraphicRoll|undefined)=>void}=$props()
  const defaults:GraphicRoll={mode:'off',startOffScreen:false,endOffScreen:false,preroll:0,easeIn:0,easeOut:0,postroll:0}
  const times=[['preroll','Пауза перед движением'],['easeIn','Разгон'],['easeOut','Торможение'],['postroll','Пауза после движения']] as const
  let error=$state('')
  function time(key:typeof times[number][0],raw:string) {
    const parsed=Number(raw)
    if (!raw.trim() || !Number.isSafeInteger(parsed) || parsed<0) {error='Укажите неотрицательное целое число микросекунд';return}
    error='';onchange({...defaults,...value,[key]:parsed})
  }
</script>
<details>
  <summary>Бегущие титры</summary>
  <label>Направление<select aria-label="Графика направление титров" value={value?.mode ?? 'off'} onchange={event=>onchange({...defaults,...value,mode:event.currentTarget.value as GraphicRoll['mode']})}>
    <option value="off">Выключено</option><option value="roll">Вверх</option><option value="crawlLeft">Влево</option><option value="crawlRight">Вправо</option>
  </select></label>
  <label><input type="checkbox" aria-label="Графика вход за кадром" checked={value?.startOffScreen ?? false} onchange={event=>onchange({...defaults,...value,startOffScreen:event.currentTarget.checked})} />Начинать за кадром</label>
  <label><input type="checkbox" aria-label="Графика выход за кадр" checked={value?.endOffScreen ?? false} onchange={event=>onchange({...defaults,...value,endOffScreen:event.currentTarget.checked})} />Заканчивать за кадром</label>
  {#each times as [key,label]}<label>{label}<input type="number" min="0" step="1" aria-label={`Графика ${key}`} value={value?.[key] ?? 0} onchange={event=>time(key,event.currentTarget.value)} /></label>{/each}
  <small>Интервалы в микросекундах.</small>
  <button class="btn ghost sm" type="button" disabled={!value} onclick={()=>onchange(undefined)}>Удалить движение титров</button>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>details {min-width:0} label {display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:.4rem} select {min-width:0;max-width:100%} input[type=number] {width:7rem;min-width:0;max-width:100%} small {display:block}</style>
