<script lang="ts">
 import {untrack} from 'svelte'
 import {writeCaptionFile,type CaptionFileFormat} from '$lib/api'
 import type {CaptionTrack, CompositionCanvas} from '$lib/composition/types'
 import {captionFileDocument} from '$lib/composition/captionFileExport'
 import {exactCaptionTicks,parseCaptionEditSeconds} from '$lib/composition/captionTime'
 import {canvasTimeGrid} from '$lib/composition/timeGrid'
 let {composition,activeId='',playheadTicks,range,exactRange,disabled=false,basename}:{
  composition:{readonly canvas:Pick<CompositionCanvas,'fps'|'fpsRate'>;readonly captionTracks?:readonly CaptionTrack[]}
  activeId?:string
  playheadTicks?:number
  range?:{startUs:number;endUs:number}
  exactRange?:{startTick:string;endTick:string}
  disabled?:boolean
  basename?:string
 }=$props()
 let format=$state<CaptionFileFormat>('srt'),dropFrame=$state(true),busy=$state(false),error=$state(''),url=$state(''),filename=$state('')
 let fromPlayhead=$state(false),endSeconds=$state('')
 const track=$derived(composition.captionTracks?.find(track=>track.id===activeId)??composition.captionTracks?.[0])
 const formats=['srt','vtt','scc','mcc','stl','ttml','dfxp'] as const
 function clear(){if(url)URL.revokeObjectURL(url);url='';filename=''}
 let disposed=false
 $effect(()=>()=>{disposed=true;clear()})
 const rangeStart = $derived(exactRange?.startTick ?? range?.startUs)
 const rangeEnd = $derived(exactRange?.endTick ?? range?.endUs)
 let lastStart = untrack(() => rangeStart)
 let lastEnd = untrack(() => rangeEnd)
 $effect(()=>{
  const start = rangeStart, end = rangeEnd
  if (lastStart === start && lastEnd === end) return
  lastStart = start; lastEnd = end
  untrack(clear)
 })
 async function exportFile(){
  if(disabled||busy||!track)return
  const start=rangeStart, endPoint=rangeEnd
  busy=true;error='';clear()
  try{
   const rate=canvasTimeGrid(composition.canvas),name=basename ?? track.name
   const offset=start !== undefined ? exactCaptionTicks(start)
     : fromPlayhead && playheadTicks !== undefined ? exactCaptionTicks(playheadTicks) : undefined
   const end=endPoint !== undefined ? exactCaptionTicks(endPoint) : endSeconds.trim() ? parseCaptionEditSeconds(endSeconds) : undefined
   if(end !== undefined && end <= (offset ?? 0n)) throw new Error('Конец экспорта должен быть позже начала')
   if(end !== undefined) exactCaptionTicks(end.toString())
   const result=await writeCaptionFile(captionFileDocument(track),format,{
    dropFrame,rate:{num:rate.numerator,den:rate.denominator},...(offset === undefined ? {} : {offsetTick:offset.toString()}),
    ...(end === undefined ? {} : {endTick:end.toString()}),
   })
   if(disposed||rangeStart!==start||rangeEnd!==endPoint)return
   const binary=atob(result.dataBase64),bytes=Uint8Array.from(binary,char=>char.charCodeAt(0))
   url=URL.createObjectURL(new Blob([bytes],{type:'application/octet-stream'}))
   filename=`${name.replace(/[\\/:*?"<>|\p{Cc}]/gu,'_')||'captions'}.${result.extension}`
  }catch(reason){error=String(reason)}finally{busy=false}
 }
</script>
<div class="export">
 <label>Формат экспорта<select aria-label="Формат экспорта субтитров" value={format} disabled={busy} onchange={event=>format=event.currentTarget.value as CaptionFileFormat}>{#each formats as item (item)}<option value={item}>{item.toUpperCase()}</option>{/each}</select></label>
 <label class="check"><input type="checkbox" aria-label="Drop-frame субтитров" checked={dropFrame} disabled={busy} onchange={event=>dropFrame=event.currentTarget.checked}/>Drop-frame</label>
 {#if !range && !exactRange && playheadTicks !== undefined}
  <label class="check"><input type="checkbox" disabled={busy} checked={fromPlayhead} onchange={event=>fromPlayhead=event.currentTarget.checked} />Экспортировать от курсора</label>
 {/if}
 {#if !range && !exactRange}<label>Конец экспорта, с<input type="text" inputmode="decimal" placeholder="До конца" disabled={busy} bind:value={endSeconds} /></label>{/if}
 <button type="button" class="btn ghost sm" disabled={disabled||busy||!track} onclick={()=>void exportFile()}>Экспортировать дорожку субтитров</button>
 {#if url}<a href={url} download={filename}>Скачать {filename}</a>{/if}
 {#if busy}<span role="status">Экспорт субтитров…</span>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</div>
<style>.export{display:flex;flex-wrap:wrap;gap:.25rem;align-items:end;font-size:.75rem}label{display:grid;gap:.15rem}.check{display:flex;align-items:center}p{width:100%;margin:.25rem 0}</style>
