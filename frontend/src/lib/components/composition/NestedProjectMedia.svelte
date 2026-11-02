<script lang="ts">
  import { nestedComposition, nestedHistory, setNestedMediaPlaceholder } from '$lib/state/nestedComposition.svelte'
  let { disabled = false }: { disabled?: boolean } = $props()
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
    {:else}<p>В проекте пока нет медиа.</p>{/each}
  </details>
{/if}
