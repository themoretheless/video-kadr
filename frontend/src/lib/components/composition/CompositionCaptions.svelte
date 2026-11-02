<script lang="ts">
 import {captionDisplaySeconds,captionMicrosecondsFloor,compareCaptionTimes,type CaptionTimestamp} from '$lib/composition/captionTime'
 import type {Composition} from '$lib/composition/types'
 import CaptionTrackSettings from './CaptionTrackSettings.svelte'
 import CaptionFileExport from './CaptionFileExport.svelte'
 import CaptionFileImport from './CaptionFileImport.svelte'
 import CaptionTrackStyle from './CaptionTrackStyle.svelte'
 import CaptionEditControls from './CaptionEditControls.svelte'
 import {upgradeCompositionCaptions,editCompositionCaptions} from '$lib/api'
 import {buildCompositionWireDocument} from '$lib/composition/payload'
 import {captionNavigationTarget} from '$lib/composition/captionNavigation'
 import {applyCaptionUpgrade} from '$lib/composition/captionUpgrade'
 import {applyCaptionEdit} from '$lib/composition/captionEdit'
 let {composition,playheadTicks=0,onchange,onseek}:{composition:Composition;playheadTicks?:number;onchange:(document:Composition)=>void;onseek?:(tick:number,clearOrdinarySelection:boolean)=>void}=$props()
 let navigationAnchor=$state<{exact:CaptionTimestamp;projected:number}|null>(null)
 let selected=$state<string[]>([]),busy=$state(false),error=$state(''),activeTrackId=$state(''),busyLabel=$state('')
 let disposed=false
 $effect(()=>()=>{disposed=true})
 $effect(()=>{
  const existing=new Set(composition.captionTracks?.flatMap(track=>track.clips.map(cue=>cue.id)))
  const retained=selected.filter(id=>existing.has(id))
  if(retained.length!==selected.length)selected=retained
 })
 function seek(time:CaptionTimestamp,clear:boolean){const projected=captionMicrosecondsFloor(time);navigationAnchor={exact:time,projected};onseek?.(projected,clear)}
 function navigate(forward:boolean){error='';try{const time=navigationAnchor?.projected===playheadTicks?navigationAnchor.exact:playheadTicks;const cue=captionNavigationTarget(composition,time,forward);selected=[cue.id];seek(cue.timelineStartTick,false)}catch(reason){error=String(reason)}}
 async function add(){
  if(busy)return
  const snapshot=composition,stamp=JSON.stringify(snapshot)
  error=''
  try {
   const tracks=snapshot.captionTracks??[],existing=tracks.find(track=>track.id===activeTrackId)??tracks[0]
   if(existing?.locked)throw new Error('Дорожка субтитров заблокирована')
   if(existing?.clips.some(cue=>compareCaptionTimes(cue.timelineStartTick,playheadTicks)<=0&&compareCaptionTimes(playheadTicks,cue.timelineEndTick)<0))throw new Error('В этой позиции уже есть субтитр')
   const track=existing??{id:`captions-${crypto.randomUUID()}`,name:'Subtitle',format:'Subtitle' as const,language:'en',syncLock:true,hidden:false,locked:false,clips:[]}
   const prepared=existing?snapshot:{...snapshot,captionTracks:[track]}
   busy=true;busyLabel='Добавление субтитра…'
   const response=await editCompositionCaptions(buildCompositionWireDocument(prepared),{kind:'add',trackId:track.id,startTick:playheadTicks,text:'Субтитр'})
   if(disposed)return
   if(JSON.stringify(composition)!==stamp)throw new Error('Композиция изменилась; повторите добавление')
   const next=applyCaptionEdit(prepared,response.composition)
   const previous=new Set(track.clips.map(cue=>cue.id))
   onchange(next);activeTrackId=track.id
   selected=next.captionTracks?.find(item=>item.id===track.id)?.clips.filter(cue=>!previous.has(cue.id)).map(cue=>cue.id)??[]
  }catch(reason){if(!disposed)error=String(reason)}finally{busy=false}
 }

 function text(id:string,text:string){onchange({...composition,captionTracks:composition.captionTracks?.map(track=>({...track,clips:track.clips.map(cue=>cue.id===id?{...cue,text}:cue)}))})}
 async function upgrade(){
  if(busy||!selected.length)return
  const snapshot=composition,stamp=JSON.stringify(snapshot);busy=true;busyLabel='Преобразование…';error=''
  try{
   const result=await upgradeCompositionCaptions(buildCompositionWireDocument(snapshot),selected)
   if(disposed)return
   if(JSON.stringify(composition)!==stamp)throw new Error('Композиция изменилась; повторите преобразование')
   onchange(applyCaptionUpgrade(snapshot,result));selected=[]
  }catch(reason){error=String(reason)}finally{busy=false}
 }
</script>
<details><summary>Субтитры</summary>
 <button type="button" class="btn ghost sm" disabled={busy} onclick={()=>void add()}>Добавить субтитр</button>
 <button type="button" class="btn ghost sm" disabled={!onseek} onclick={()=>navigate(false)}>Предыдущий субтитр</button>
 <button type="button" class="btn ghost sm" disabled={!onseek} onclick={()=>navigate(true)}>Следующий субтитр</button>
 <CaptionTrackSettings {composition} activeId={activeTrackId} disabled={busy} {onchange} onactive={id=>activeTrackId=id} />
 {#each composition.captionTracks??[] as track (track.id)}
  <fieldset disabled={busy}><legend>{track.name}{track.locked?' · Заблокировано':''}</legend>
   <fieldset disabled={track.locked}><CaptionTrackStyle style={track.style} onchange={style=>onchange({...composition,captionTracks:composition.captionTracks?.map(item=>item.id===track.id?{...item,style}:item)})} /></fieldset>
   {#each track.clips as cue (cue.id)}
    <label><input type="checkbox" aria-label={`Выбрать субтитр ${cue.id}`} checked={selected.includes(cue.id)} onchange={event=>selected=event.currentTarget.checked?[...selected,cue.id]:selected.filter(id=>id!==cue.id)} />
     <span>{captionDisplaySeconds(cue.timelineStartTick).toFixed(3)}–{captionDisplaySeconds(cue.timelineEndTick).toFixed(3)} с</span>
     <button type="button" class="btn ghost sm" aria-label={`Перейти к субтитру ${cue.id}`} disabled={!onseek} onclick={()=>{selected=[cue.id];seek(cue.timelineStartTick,true)}}>Перейти</button>
     <textarea disabled={track.locked} aria-label={`Текст субтитра ${cue.id}`} value={cue.text} onchange={event=>text(cue.id,event.currentTarget.value)}></textarea>
    </label>
   {/each}
  </fieldset>
 {/each}
 <CaptionFileImport {composition} {onchange} onactive={id=>activeTrackId=id} />
 <CaptionFileExport {composition} activeId={activeTrackId} />
 <CaptionEditControls {composition} {selected} {playheadTicks} {onchange} onselection={ids=>selected=ids} />
 <button type="button" class="btn ghost sm" disabled={busy||!selected.length} onclick={()=>void upgrade()}>Преобразовать в графику</button>
 {#if busy}<p role="status">{busyLabel}</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>details{font-size:.75rem}summary{cursor:pointer}fieldset{border:0;padding:.25rem 0;margin:0}label{display:grid;grid-template-columns:auto 1fr;gap:.25rem;align-items:center;margin:.25rem 0}textarea{grid-column:1/-1;min-height:2.5rem;width:100%;resize:vertical}p{margin:.25rem 0}</style>
