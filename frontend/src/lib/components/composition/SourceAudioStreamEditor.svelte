<script lang="ts">
 import {getSourceAudioStreams,type SourceAudioStream} from '$lib/api'
 import {authState} from '$lib/state/auth.svelte'
 let {sourceId,value,disabled=false,onchange}:{sourceId:string;value?:number;disabled?:boolean;onchange:(value:number|undefined)=>void}=$props()
 let catalog=$state<{sourceId:string;token:string|null;streams:SourceAudioStream[]}|null>(null)
 let loading=$state(false)
 let error=$state('')
 let requestGeneration=0
 let streams=$derived(catalog?.sourceId===sourceId&&catalog?.token===authState.token?catalog?.streams:undefined)
 async function load(){
  const requestedSource=sourceId,token=authState.token;if(!requestedSource)return
  const generation=++requestGeneration;loading=true;error=''
  try{const result=await getSourceAudioStreams(requestedSource,token??undefined);if(generation===requestGeneration&&sourceId===requestedSource&&authState.token===token){if(result.sourceId!==requestedSource)throw new Error('Источник ответа изменился');catalog={sourceId:requestedSource,token,streams:result.streams}}}
  catch(reason){if(generation===requestGeneration&&sourceId===requestedSource&&authState.token===token)error=reason instanceof Error?reason.message:String(reason)}
  finally{if(generation===requestGeneration)loading=false}
 }
 let disposed=false
 $effect(()=>()=>{disposed=true;requestGeneration++})
 function edit(raw:string){if(disabled||disposed)return;if(raw!==''&&(!/^\d+$/.test(raw)||Number(raw)>0xffffffff))return;onchange(raw===''?undefined:Number(raw))}
</script>
<fieldset {disabled}>
   <button type="button" disabled={loading} onclick={()=>void load()}>{loading?'Чтение потоков…':'Загрузить список аудиодорожек'}</button>
   {#if streams}
    <label>Дорожка <select aria-label="Аудиодорожка источника" disabled={disabled} value={value??''} onchange={event=>edit(event.currentTarget.value)}>
     <option value="">По умолчанию</option>
     {#if value!==undefined&&!streams.some(stream=>stream.index===value)}<option value={value}>#{value} · отсутствует в списке</option>{/if}
     {#each streams as stream(stream.index)}<option value={stream.index}>#{stream.index} · {[stream.title,stream.language,stream.codec,stream.channels?`${stream.channels} кан.`:'',stream.sampleRate?`${stream.sampleRate} Гц`:'',stream.default?'основная':''].filter(Boolean).join(' · ')}</option>{/each}
    </select></label>
    {#if streams.length===0}<p>Аудиопотоки не найдены.</p>{/if}
   {/if}
   {#if error}<p role="alert">{error}</p>{/if}
   <label>Индекс потока <input aria-label="Индекс аудиопотока" type="number" min="0" max="4294967295" step="1" placeholder="По умолчанию" disabled={disabled} value={value??''} onchange={event=>edit(event.currentTarget.value)} /></label>
   <button type="button" disabled={disabled||value===undefined} onclick={()=>edit('')}>Использовать дорожку по умолчанию</button>
</fieldset>
<p>Абсолютный индекс потока в файле, начиная с 0, с учётом видеопотоков. Пустое поле выбирает первую аудиодорожку. Наличие и возможность декодирования проверяются при воспроизведении и экспорте.</p>
<style>fieldset{border:0;padding:0.5rem 0}label{display:flex;gap:0.5rem;align-items:center;margin-bottom:0.5rem}input{min-width:0;width:9rem}p{font-size:0.85rem}</style>
