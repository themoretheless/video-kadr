<script lang="ts">
  import { nestedComposition, nestedHistory, setNestedMediaPlaceholder, removeNestedMedia, fillNestedMedia } from '$lib/state/nestedComposition.svelte'
  import type { ConcatImportSource } from '$lib/composition/concatProjectImport'
  let { disabled = false, token = null, sources = [] }: { disabled?: boolean; token?: string|null; sources?:readonly ConcatImportSource[] } = $props()
  let choices=$state<Record<string,string>>({})
  let epoch=0
  $effect(()=>{void token;void sources;return()=>{epoch++}})
  async function fill(id:string){
    const replacement=choices[id],auth=token,serial=epoch
    if(!auth||!replacement||disabled||nestedHistory.busy)return
    await fillNestedMedia(id,replacement,auth,()=>{if(serial!==epoch||auth!==token)throw new Error('Контекст заполнения слота изменился')})
  }
  const project = $derived(nestedComposition.document)
  const items = $derived.by(() => {
    const result: { id: string; name: string; placeholder: boolean; timelines: number }[] = []
    if (!project) return []
    for (const [id, item] of Object.entries(project.projectMedia ?? {})) result.push({ id, name: item.name, placeholder: item.placeholder === true, timelines: 0 })
    for (const node of Object.values(project.compositions)) for (const source of Object.values(node.content.sources)) {
      const item = result.find(item => item.id === source.id) ?? { id: source.id, name: node.content.metadata?.sourceNames?.[source.id] ?? source.id, placeholder: false, timelines: 0 }
      if (!result.includes(item)) result.push(item)
      item.timelines++
    }
    return result
  })
</script>
{#if project}
  <details>
    <summary>Медиа проекта и слоты шаблона</summary>
    {#each items as item (item.id)}
      <label><input type="checkbox" aria-label={`Слот шаблона: ${item.name}`} checked={item.placeholder} disabled={disabled || nestedHistory.busy} onchange={event => { void setNestedMediaPlaceholder(item.id, event.currentTarget.checked) }} />{item.name} <span>Таймлайнов: {item.timelines}</span></label>
      {#if item.placeholder}
        <label>Новый файл <select aria-label={`Замена медиа слота ${item.name}`} value={choices[item.id]??''} disabled={disabled||nestedHistory.busy||!token} onchange={event=>{choices={...choices,[item.id]:event.currentTarget.value}}}><option value="">Выберите файл из библиотеки</option>{#each sources as entry(entry.source.id)}<option value={entry.source.id}>{entry.label}</option>{/each}</select></label>
        <button disabled={disabled||nestedHistory.busy||!token||!choices[item.id]} aria-label={`Заполнить слот проекта: ${item.name}`} onclick={()=>{void fill(item.id)}}>Заполнить слот на всех таймлайнах</button>
      {/if}
      <button disabled={disabled || nestedHistory.busy} aria-label={`Удалить медиа и все его клипы: ${item.name}`} onclick={() => { void removeNestedMedia(item.id) }}>Удалить медиа и его клипы</button>
    {:else}<p>В проекте пока нет медиа.</p>{/each}
  </details>
{/if}
