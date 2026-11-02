<script lang="ts">
 import {writeCaptionFile,type CaptionFileFormat} from '$lib/api'
 import type {Composition} from '$lib/composition/types'
 import {captionFileDocument} from '$lib/composition/captionFileExport'
 import {canvasTimeGrid} from '$lib/composition/timeGrid'
 let {composition,activeId=''}:{composition:Composition;activeId?:string}=$props()
 let format=$state<CaptionFileFormat>('srt'),dropFrame=$state(true),busy=$state(false),error=$state(''),url=$state(''),filename=$state('')
 const track=$derived(composition.captionTracks?.find(track=>track.id===activeId)??composition.captionTracks?.[0])
 const formats=['srt','vtt','scc','mcc','stl','ttml','dfxp'] as const
 function clear(){if(url)URL.revokeObjectURL(url);url='';filename=''}
 let disposed=false
 $effect(()=>()=>{disposed=true;clear()})
 async function exportFile(){
  if(busy||!track)return
  busy=true;error='';clear()
  try{
   const rate=canvasTimeGrid(composition.canvas),name=track.name
   const result=await writeCaptionFile(captionFileDocument(track),format,{dropFrame,rate:{num:rate.numerator,den:rate.denominator}})
   if(disposed)return
   const binary=atob(result.dataBase64),bytes=Uint8Array.from(binary,char=>char.charCodeAt(0))
   url=URL.createObjectURL(new Blob([bytes],{type:'application/octet-stream'}))
   filename=`${name.replace(/[\\/:*?"<>|\u0000-\u001f]/g,'_')||'captions'}.${result.extension}`
  }catch(reason){error=String(reason)}finally{busy=false}
 }
</script>
<div class="export">
 <label>Формат экспорта<select aria-label="Формат экспорта субтитров" value={format} disabled={busy} onchange={event=>format=event.currentTarget.value as CaptionFileFormat}>{#each formats as item}<option value={item}>{item.toUpperCase()}</option>{/each}</select></label>
 <label class="check"><input type="checkbox" aria-label="Drop-frame субтитров" checked={dropFrame} disabled={busy} onchange={event=>dropFrame=event.currentTarget.checked}/>Drop-frame</label>
 <button type="button" class="btn ghost sm" disabled={busy||!track} onclick={()=>void exportFile()}>Экспортировать дорожку субтитров</button>
 {#if url}<a href={url} download={filename}>Скачать {filename}</a>{/if}
 {#if busy}<span role="status">Экспорт субтитров…</span>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</div>
<style>.export{display:flex;flex-wrap:wrap;gap:.25rem;align-items:end;font-size:.75rem}label{display:grid;gap:.15rem}.check{display:flex;align-items:center}p{width:100%;margin:.25rem 0}</style>
