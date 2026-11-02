<script lang="ts">
 import {prepareCaptionFileImport} from '$lib/api'
 import type {Composition} from '$lib/composition/types'
 import {applyPreparedCaptionImport} from '$lib/composition/captionFileImport'
 import {canvasTimeGrid} from '$lib/composition/timeGrid'
 let {composition,onchange,onactive}:{composition:Composition;onchange:(document:Composition)=>void;onactive:(id:string)=>void}=$props()
 let busy=$state(false),error=$state(''),warnings=$state<readonly string[]>([]),disposed=false
 $effect(()=>()=>{disposed=true})
 function encoded(file:File):Promise<string>{return new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>{const value=String(reader.result),comma=value.indexOf(',');if(comma<0)reject(new Error('Не удалось прочитать файл'));else resolve(value.slice(comma+1))};reader.onerror=()=>reject(new Error('Не удалось прочитать файл'));reader.readAsDataURL(file)})}
 async function importFile(event:Event){
  const input=event.currentTarget as HTMLInputElement,file=input.files?.[0]
  if(!file||busy)return
  const snapshot=composition,stamp=JSON.stringify(snapshot);busy=true;error='';warnings=[]
  try{
   const rate=canvasTimeGrid(snapshot.canvas),data=await encoded(file)
   if(disposed)return
   const prepared=await prepareCaptionFileImport(data,file.name,{num:rate.numerator,den:rate.denominator})
   if(disposed)return
   if(JSON.stringify(composition)!==stamp)throw new Error('Композиция изменилась; повторите импорт')
   const result=applyPreparedCaptionImport(snapshot,prepared)
   onchange(result.composition);onactive(result.trackId);warnings=prepared.document.warnings??[]
  }catch(reason){if(!disposed)error=String(reason)}finally{busy=false;input.value=''}
 }
</script>
<label class="import">Импорт субтитров<input type="file" aria-label="Импорт файла субтитров" accept=".srt,.vtt,.scc,.mcc,.stl,.ttml,.dfxp" disabled={busy} onchange={event=>void importFile(event)} /></label>
{#if busy}<p role="status">Импорт субтитров…</p>{/if}
{#if error}<p role="alert">{error}</p>{/if}
{#each warnings as warning}<p class="warning">{warning}</p>{/each}
<style>.import{display:grid;gap:.15rem;font-size:.75rem;margin:.25rem 0}input{max-width:100%}p{font-size:.75rem;margin:.25rem 0}</style>
