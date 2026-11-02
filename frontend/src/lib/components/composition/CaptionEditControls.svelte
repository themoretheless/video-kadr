<script lang="ts">
 import {captionDisplaySeconds,captionFrameDelta} from '$lib/composition/captionTime'
 import type {Composition} from '$lib/composition/types'
 import {editCompositionCaptions,type CaptionEditOperation,type CaptionEditContext,type CaptionCommandTimeInput} from '$lib/api'
 import {buildCompositionWireDocument} from '$lib/composition/payload'
 import {applyCaptionEdit} from '$lib/composition/captionEdit'
 let {composition,selected,playheadTicks,onchange,onselection}:{composition:Composition;selected:readonly string[];playheadTicks:number;onchange:(document:Composition)=>void;onselection?:(ids:string[])=>void}=$props()
 let busy=$state(false),error=$state(''),ripple=$state(false),delta=$state(1),start=$state(0),end=$state(0),speaker=$state(''),deltaUnit=$state<'seconds'|'frames'>('seconds'),timingUnit=$state<'seconds'|'frames'|'timecode'>('seconds'),authoredStart=$state(''),authoredEnd=$state(''),dropFrame=$state(false)
 const cue=$derived(selected.length===1?composition.captionTracks?.flatMap(track=>track.clips).find(cue=>cue.id===selected[0]):undefined)
 $effect(()=>{start=captionDisplaySeconds(cue?.timelineStartTick??0);end=captionDisplaySeconds(cue?.timelineEndTick??0);speaker=cue?.speaker??'';authoredStart='';authoredEnd=''})
 function tick(seconds:number):number {const tick=Math.round(seconds*1_000_000);if(!Number.isFinite(seconds)||!Number.isSafeInteger(tick))throw new Error('Некорректное время субтитра');return tick}
 async function edit(operation:CaptionEditOperation,context?:CaptionEditContext){
  if(busy)return
  const snapshot=composition,stamp=JSON.stringify(snapshot);busy=true;error=''
  try {
   const response=context?await editCompositionCaptions(buildCompositionWireDocument(snapshot),operation,undefined,context):await editCompositionCaptions(buildCompositionWireDocument(snapshot),operation)
   if(JSON.stringify(composition)!==stamp)throw new Error('Композиция изменилась; повторите правку')
   const next=applyCaptionEdit(snapshot,response.composition)
   const remaining=new Set(next.captionTracks?.flatMap(track=>track.clips.map(cue=>cue.id)))
   onchange(next)
   if(operation.kind==='split')onselection?.(next.captionTracks?.flatMap(track=>track.clips).filter(cue=>!snapshot.captionTracks?.some(track=>track.clips.some(original=>original.id===cue.id))).map(cue=>cue.id)??[])
   else onselection?.(selected.filter(id=>remaining.has(id)))
  }catch(reason){error=String(reason)}finally{busy=false}
 }
 function authoredTime(value:string):CaptionCommandTimeInput|undefined {
  if(!value.trim())return undefined
  if(timingUnit==='timecode')return {unit:'timecode',value:value.trim()}
  const frame=Number(value)
  if(!/^[+-]?\d+$/.test(value.trim())||!Number.isSafeInteger(frame))throw new Error('Введите целое число кадров')
  return {unit:'frames',value:frame}
 }
 function timing(){try{
  if(!cue)return
  if(timingUnit==='seconds')void edit({kind:'times',id:cue.id,...(start===captionDisplaySeconds(cue.timelineStartTick)?{}:{startTick:tick(start)}),...(end===captionDisplaySeconds(cue.timelineEndTick)?{}:{endTick:tick(end)})})
  else {
   const startInput=authoredTime(authoredStart),endInput=authoredTime(authoredEnd)
   if(!startInput&&!endInput)return
   void edit({kind:'times',id:cue.id,...(startInput?{startInput}:{}),...(endInput?{endInput}:{})},timingUnit==='timecode'?{playheadTick:playheadTicks,dropFrame}:undefined)
  }
 }catch(reason){error=String(reason)}}

 function shift(kind:'move'|'trim',inEdge=false){try{const deltaTicks=deltaUnit==='frames'?captionFrameDelta(delta,composition.canvas.fpsRate??{numerator:Math.round(composition.canvas.fps*1000),denominator:1000}):tick(delta);void edit(kind==='move'?{kind,ids:selected,deltaTicks}:{kind,id:selected[0],inEdge,deltaTicks})}catch(reason){error=String(reason)}}
</script>
<fieldset disabled={busy||!selected.length} class="controls"><legend>Правка выбранных субтитров</legend>
 <div class="row"><button type="button" class="btn ghost sm" disabled={selected.length!==1} onclick={()=>void edit({kind:'split',id:selected[0],timeTick:playheadTicks})}>Разрезать субтитр</button><button type="button" class="btn ghost sm" disabled={selected.length<2} onclick={()=>void edit({kind:'merge',ids:selected})}>Объединить субтитры</button></div>
 {#if cue}<label>Ввод времени<select aria-label="Единицы времени субтитра" value={timingUnit} onchange={event=>{timingUnit=event.currentTarget.value as 'seconds'|'frames'|'timecode';authoredStart='';authoredEnd=''}}><option value="seconds">Секунды</option><option value="frames">Кадры</option><option value="timecode">Таймкод</option></select></label><div class="times">
  {#if timingUnit==='seconds'}<label>Начало, с<input aria-label="Начало выбранного субтитра" type="number" min="0" step="0.001" bind:value={start} /></label>
  <label>Конец, с<input aria-label="Конец выбранного субтитра" type="number" min="0" step="0.001" bind:value={end} /></label>
  {:else}<label>Начало<input aria-label="Точное начало выбранного субтитра" placeholder="Не менять" inputmode={timingUnit==='frames'?'numeric':'text'} bind:value={authoredStart} /></label><label>Конец<input aria-label="Точный конец выбранного субтитра" placeholder="Не менять" inputmode={timingUnit==='frames'?'numeric':'text'} bind:value={authoredEnd} /></label>{/if}
  {#if timingUnit==='timecode'}<label class="check"><input aria-label="Drop-frame таймкод субтитра" type="checkbox" bind:checked={dropFrame} />Drop-frame</label>{/if}
  <button type="button" class="btn ghost sm" onclick={timing}>Применить время субтитра</button>
  <label>Спикер<input aria-label="Спикер выбранного субтитра" bind:value={speaker} /></label><button type="button" class="btn ghost sm" onclick={()=>void edit({kind:'text',id:cue.id,speaker:speaker.trim()||null})}>Применить спикера</button>
 </div>{/if}
 <div class="row"><label>Единицы сдвига<select aria-label="Единицы сдвига субтитров" value={deltaUnit} onchange={event=>deltaUnit=event.currentTarget.value==='frames'?'frames':'seconds'}><option value="seconds">Секунды</option><option value="frames">Кадры</option></select></label><label>Сдвиг<input aria-label={deltaUnit==='frames'?'Сдвиг субтитров в кадрах':'Сдвиг субтитров в секундах'} type="number" step={deltaUnit==='frames'?1:0.001} bind:value={delta} /></label></div>
 <div class="row"><button type="button" class="btn ghost sm" onclick={()=>shift('move')}>Сдвинуть субтитры</button><button type="button" class="btn ghost sm" disabled={selected.length!==1} onclick={()=>shift('trim',true)}>Обрезать начало</button><button type="button" class="btn ghost sm" disabled={selected.length!==1} onclick={()=>shift('trim',false)}>Обрезать конец</button></div>
 <label class="check"><input aria-label="Удалять субтитры со сдвигом" type="checkbox" bind:checked={ripple} />Закрыть промежутки после удаления</label>
 <button type="button" class="btn ghost sm" onclick={()=>void edit({kind:'delete',ids:selected,ripple})}>Удалить субтитры</button>
</fieldset>
{#if busy}<p role="status">Правка субтитров…</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
<style>.controls{border:0;padding:.35rem 0;margin:0;font-size:.75rem}.row{display:flex;flex-wrap:wrap;gap:.25rem;margin:.25rem 0}.times{display:grid;grid-template-columns:1fr 1fr;gap:.25rem}label{display:grid;gap:.15rem;margin:.25rem 0;min-width:0}input{min-width:0;width:100%}.check{display:flex;align-items:center}.check input{width:auto}p{font-size:.75rem;margin:.25rem 0}</style>
