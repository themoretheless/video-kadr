<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedHistory,nestedAgentGeneration,addNestedFontFile,removeNestedFontFile} from '$lib/state/nestedComposition.svelte'
 import type {NativeProjectFont} from '$lib/api'
 let {disabled=false}:{disabled?:boolean}=$props()
 let reading=$state(false),error=$state(''),alive=true
 const resources=$derived(nestedComposition.document?.fonts??[])
 const blocked=$derived(disabled||reading||nestedHistory.busy||Boolean(agentSession.lock)||!nestedComposition.document)
 $effect(()=>{void nestedComposition.document;error=''})
 $effect(()=>()=>{alive=false})
 async function add(file:File|undefined){
  if(!file||blocked)return
  const document=nestedComposition.document,projectId=nestedComposition.projectId,generation=nestedAgentGeneration()
  const current=()=>alive&&projectId===nestedComposition.projectId&&generation===nestedAgentGeneration()
  reading=true;error=''
  try{
   if(file.size>8*1024*1024)throw new Error('Файл шрифта превышает 8 МиБ')
   const bytes=new Uint8Array(await file.arrayBuffer())
   if(!current())return
   if(document!==nestedComposition.document)throw new Error('Проект изменился; выберите файл ещё раз')
   if(bytes.length>8*1024*1024)throw new Error('Файл шрифта превышает 8 МиБ')
   let binary='';for(let at=0;at<bytes.length;at+=32768)binary+=String.fromCharCode(...bytes.subarray(at,at+32768))
   if(disabled||nestedHistory.busy)throw new Error('Редактор занят; повторите добавление файла')
   await addNestedFontFile(btoa(binary))
   if(current()&&nestedHistory.error)error=nestedHistory.error
  }catch(reason){if(current())error=reason instanceof Error?reason.message:String(reason)}
  finally{if(alive)reading=false}
 }
 async function remove(index:number,resource:NativeProjectFont){
  if(blocked)return
  const generation=nestedAgentGeneration(),projectId=nestedComposition.projectId;error=''
  await removeNestedFontFile(index,resource)
  if(alive&&generation===nestedAgentGeneration()&&projectId===nestedComposition.projectId&&nestedHistory.error)error=nestedHistory.error
 }
</script>
<section aria-label="Файлы шрифтов проекта">
 <fieldset disabled={blocked}>
  <label>Добавить шрифт <input type="file" accept=".ttf,.otf,.ttc" aria-label="Добавить файл шрифта в проект" onchange={event=>{const file=event.currentTarget.files?.[0];event.currentTarget.value='';void add(file)}} /></label>
  {#each resources as resource,index (index)}
   <div>{resource.family} <button type="button" class="btn ghost sm" aria-label={`Удалить файл шрифта ${resource.family} ${index+1}`} onclick={()=>void remove(index,resource)}>Удалить файл</button></div>
  {/each}
 </fieldset>
 <p>Файлы сохраняются в проекте. Добавление и удаление можно отменить.</p>
 {#if reading}<p role="status">Читаю файл шрифта…</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</section>
<style>fieldset{border:0;padding:0;display:grid;gap:.4rem}label,fieldset div{display:flex;align-items:center;gap:.5rem;flex-wrap:wrap}p{margin:.3rem 0}</style>
