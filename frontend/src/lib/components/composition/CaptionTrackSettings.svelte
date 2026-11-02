<script lang="ts">
 import type {CaptionTrack,Composition} from '$lib/composition/types'
 let {composition,activeId='',disabled=false,onchange,onactive}:{composition:Composition;activeId?:string;disabled?:boolean;onchange:(document:Composition)=>void;onactive:(id:string)=>void}=$props()
 let newFormat=$state<NonNullable<CaptionTrack['format']>>('Subtitle')
 const tracks=$derived(composition.captionTracks??[])
 const track=$derived(tracks.find(track=>track.id===activeId)??tracks[0])
 const formats=['Subtitle','CEA-608','CEA-708','Teletext'] as const
 function update(patch:Partial<CaptionTrack>){if(track)onchange({...composition,captionTracks:tracks.map(item=>item.id===track.id?{...item,...patch}:item)})}
 function create(){
  const next:CaptionTrack={id:`captions-${crypto.randomUUID()}`,name:newFormat,format:newFormat,language:'en',syncLock:true,hidden:false,locked:false,clips:[],...(newFormat==='CEA-608'?{style:{backgroundColor:'#000000ff'}}:{})}
  onchange({...composition,captionTracks:[next,...tracks]});onactive(next.id)
 }
 function showAll(hidden:boolean){onchange({...composition,captionTracks:tracks.map(item=>({...item,hidden}))})}
 function remove(){if(!track||track.locked)return;const remaining=tracks.filter(item=>item.id!==track.id);onchange({...composition,captionTracks:remaining});onactive(remaining[0]?.id??'')}
</script>
<fieldset disabled={disabled} class="settings">
 <div class="row"><label>Новая дорожка<select aria-label="Формат новой дорожки субтитров" value={newFormat} onchange={event=>newFormat=event.currentTarget.value as NonNullable<CaptionTrack['format']>}>{#each formats as format}<option value={format}>{format}</option>{/each}</select></label><button type="button" class="btn ghost sm" onclick={create}>Создать дорожку субтитров</button></div>
 {#if track}
  <label>Дорожка<select aria-label="Активная дорожка субтитров" value={track.id} onchange={event=>onactive(event.currentTarget.value)}>{#each tracks as item}<option value={item.id}>{item.name}</option>{/each}</select></label>
  <div class="row"><button type="button" class="btn ghost sm" onclick={()=>showAll(false)}>Показать все субтитры</button><button type="button" class="btn ghost sm" onclick={()=>showAll(true)}>Скрыть все субтитры</button></div>
  <label class="check"><input aria-label="Видимость дорожки субтитров" type="checkbox" checked={!track.hidden} onchange={event=>update({hidden:!event.currentTarget.checked})} />Показывать дорожку</label>
  <label class="check"><input aria-label="Блокировка дорожки субтитров" type="checkbox" checked={track.locked??false} onchange={event=>update({locked:event.currentTarget.checked})} />Заблокировать дорожку</label>
  <fieldset disabled={track.locked} class="grid">
   <label>Название<input aria-label="Название дорожки субтитров" value={track.name} onchange={event=>update({name:event.currentTarget.value})} /></label>
   <label>Формат<select aria-label="Формат дорожки субтитров" value={track.format??'Subtitle'} onchange={event=>update({format:event.currentTarget.value as NonNullable<CaptionTrack['format']>})}>{#each formats as format}<option value={format}>{format}</option>{/each}</select></label>
   <label>Язык<input aria-label="Язык дорожки субтитров" value={track.language??'en'} onchange={event=>update({language:event.currentTarget.value})} /></label>
   <label class="check"><input aria-label="Sync-lock дорожки субтитров" type="checkbox" checked={track.syncLock??true} onchange={event=>update({syncLock:event.currentTarget.checked})} />Sync-lock</label>
   <button type="button" class="btn ghost sm" onclick={remove}>Удалить дорожку субтитров</button>
  </fieldset>
 {/if}
</fieldset>
<style>.settings,fieldset{border:0;padding:.25rem 0;margin:0}.grid{display:grid;grid-template-columns:1fr 1fr;gap:.25rem}.row{display:flex;flex-wrap:wrap;gap:.25rem;align-items:end}label{display:grid;gap:.15rem;font-size:.75rem;min-width:0}input,select{min-width:0;width:100%}.check{display:flex;align-items:center}.check input{width:auto}</style>
