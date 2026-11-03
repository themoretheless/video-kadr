<script lang="ts">
  import { nestedComposition, nestedHistory, setNestedMediaPlaceholder, removeNestedMedia, fillNestedMedia, fillAllNestedMedia } from '$lib/state/nestedComposition.svelte'
  import {inspectNativeTemplate,type NativeTemplateInfo} from '$lib/api'
  import type { ConcatImportSource } from '$lib/composition/concatProjectImport'
  let { disabled = false, token = null, sources = [] }: { disabled?: boolean; token?: string|null; sources?:readonly ConcatImportSource[] } = $props()
  let choices=$state<Record<string,string>>({})
  let expanded=$state(false)
  let templateInfo=$state<NativeTemplateInfo|null>(null)
  let templateError=$state('')
  let epoch=0
  $effect(()=>{void token;void sources;return()=>{epoch++}})
  async function fill(id:string){
    const replacement=choices[id],auth=token,serial=epoch
    if(!auth||!replacement||disabled||nestedHistory.busy)return
    await fillNestedMedia(id,replacement,auth,()=>{if(serial!==epoch||auth!==token)throw new Error('Контекст заполнения слота изменился')})
  }
  async function fillAll(){
    const auth=token,serial=epoch
    if(!auth||disabled||nestedHistory.busy)return
    const fills=Object.fromEntries(items.filter(item=>item.placeholder).map(item=>[item.id,choices[item.id]??'']))
    await fillAllNestedMedia(fills,auth,()=>{if(serial!==epoch||auth!==token)throw new Error('Контекст заполнения слотов изменился')})
  }
  const project = $derived(nestedComposition.document)
  $effect(()=>{
    const current=project,active=nestedComposition.activeCompositionId??current?.root
    templateInfo=null;templateError=''
    if(!expanded||!current||!active)return
    const controller=new AbortController()
    void inspectNativeTemplate(current,active,controller.signal).then(info=>{if(!controller.signal.aborted)templateInfo=info}).catch(error=>{if(!controller.signal.aborted)templateError=error instanceof Error?error.message:String(error)})
    return()=>controller.abort()
  })
  const items = $derived.by(() => {
    const result: { id: string; name: string; placeholder: boolean; timelines: number }[] = []
    if (!project) return []
    for (const [id, item] of Object.entries(project.projectMedia ?? {})) result.push({ id, name: item.name, placeholder: item.placeholder === true, timelines: 0 })
    for (const node of Object.values(project.compositions)) for (const source of Object.values(node.content.sources)) {
      const item = result.find(item => item.id === source.id) ?? { id: source.id, name: node.content.metadata?.sourceNames?.[source.id] ?? source.id, placeholder: false, timelines: 0 }
      if (!result.includes(item)) result.push(item)
      item.timelines++
    }
    const order=new Map(templateInfo?.slots.map((slot,index)=>[slot.mediaId,index])??[])
    return result.sort((a,b)=>(order.get(a.id)??Number.MAX_SAFE_INTEGER)-(order.get(b.id)??Number.MAX_SAFE_INTEGER))
  })
</script>
{#if project}
  <details ontoggle={event=>{expanded=event.currentTarget.open}}>
    <summary>Медиа проекта и слоты шаблона</summary>
    {#if templateError}<p role="alert">{templateError}</p>{/if}
    {#if items.some(item=>item.placeholder)}
      <button disabled={disabled||nestedHistory.busy||!token||items.some(item=>item.placeholder&&!choices[item.id])} onclick={()=>{void fillAll()}}>Заполнить все слоты шаблона</button>
    {/if}
    {#each items as item (item.id)}
      <label><input type="checkbox" aria-label={`Слот шаблона: ${item.name}`} checked={item.placeholder} disabled={disabled || nestedHistory.busy} onchange={event => { void setNestedMediaPlaceholder(item.id, event.currentTarget.checked) }} />{item.name} <span>Таймлайнов: {item.timelines}</span></label>
      {#if item.placeholder}
        {@const slot=templateInfo?.slots.find(slot=>slot.mediaId===item.id)}
        {#if slot}<p>Тип: {slot.kind==='video'?'Видео':slot.kind==='audio'?'Аудио':'Изображение'} · Длительность на активном таймлайне: {slot.seconds.toLocaleString('ru-RU',{maximumFractionDigits:3})} с</p>{/if}
        <label>Новый файл <select aria-label={`Замена медиа слота ${item.name}`} value={choices[item.id]??''} disabled={disabled||nestedHistory.busy||!token} onchange={event=>{choices={...choices,[item.id]:event.currentTarget.value}}}><option value="">Выберите файл из библиотеки</option>{#each sources as entry(entry.source.id)}<option value={entry.source.id}>{entry.label}</option>{/each}</select></label>
        <button disabled={disabled||nestedHistory.busy||!token||!choices[item.id]} aria-label={`Заполнить слот проекта: ${item.name}`} onclick={()=>{void fill(item.id)}}>Заполнить слот на всех таймлайнах</button>
      {/if}
      <button disabled={disabled || nestedHistory.busy} aria-label={`Удалить медиа и все его клипы: ${item.name}`} onclick={() => { void removeNestedMedia(item.id) }}>Удалить медиа и его клипы</button>
    {:else}<p>В проекте пока нет медиа.</p>{/each}
  </details>
{/if}
