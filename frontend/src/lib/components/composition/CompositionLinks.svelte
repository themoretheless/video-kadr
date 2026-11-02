<script lang="ts">
  import { compositionState, commitDocument, makeId, selectCompositionClip, toggleCompositionLinkOverride } from '$lib/state/composition/core.svelte.js'
  import { duplicateClip, findClipLocation } from '$lib/composition/commands.js'
  import { clipEndTicks } from '$lib/composition/types.js'
  import { createCompositionLink, removeCompositionLink } from '$lib/composition/links.js'
  let members = $state<string[]>([])
  let reassign = $state(false)
  let error = $state('')
  const clips = $derived(compositionState.document.tracks.flatMap((track) => track.clips.map((clip) => ({ clip, track }))))
  function create(): void {
    try { commitDocument(createCompositionLink(compositionState.document, makeId('link'), members, reassign)); members = []; error = '' }
    catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }
  function remove(id: string): void {
    try { commitDocument(removeCompositionLink(compositionState.document, id)); error = '' }
    catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }
  function copy(linkId: string): void {
    try {
      const link = compositionState.document.links?.find((link) => link.id === linkId)
      if (!link) throw new Error('Связь не найдена')
      const locations = link.memberIds.map((id) => findClipLocation(compositionState.document, id))
      const seed = locations[0]!
      const delta = Math.max(...locations.map(({ clip }) => clipEndTicks(clip))) - Math.min(...locations.map(({ clip }) => clip.timelineStartTicks))
      const id = makeId('copy')
      commitDocument(duplicateClip(compositionState.document, seed.clip.id, { id, linkId: makeId('link'), timelineStartTicks: seed.clip.timelineStartTicks + delta }))
      selectCompositionClip(seed.track.id, id)
      error = ''
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }
</script>

<details class="composition-tool-section">
  <summary>Связи клипов</summary>
  <div class="composition-tool-body">
    <p>Связанные клипы перемещаются и разделяются вместе. Выберите участников на любых дорожках.</p>
    <button class="btn ghost sm" type="button" aria-pressed={compositionState.ui.linkOverride} onclick={toggleCompositionLinkOverride}>Править клипы отдельно</button>
    {#if compositionState.ui.linkOverride}<p role="status">Связи временно отключены · Alt+Shift+G</p>{/if}
    <div class="members">
      {#each clips as { clip, track } (clip.id)}
        <label><input type="checkbox" disabled={track.locked} checked={members.includes(clip.id)} onchange={(event) => { members = event.currentTarget.checked ? [...members, clip.id] : members.filter((id) => id !== clip.id) }} /> {track.name} · {clip.kind === 'text' ? clip.text : clip.kind === 'color' ? `Цвет ${clip.color}` : clip.kind === 'graphic' ? 'Графика' : clip.sourceId} · {(clip.timelineStartTicks / 1_000_000).toFixed(3)} с</label>
      {/each}
    </div>
    <label><input type="checkbox" bind:checked={reassign} /> Переназначить участников из других связей</label>
    <button class="btn ghost sm" type="button" disabled={members.filter((id) => clips.some((item) => item.clip.id === id)).length < 2} onclick={create}>Связать выбранные клипы</button>
    {#each compositionState.document.links ?? [] as link (link.id)}
      <div>{link.memberIds.length} клипа · {link.id} <button class="btn ghost sm" type="button" onclick={() => copy(link.id)}>Копировать связь после исходных</button> <button class="btn ghost sm" type="button" onclick={() => remove(link.id)}>Разорвать связь</button></div>
    {/each}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
</details>

<style>
  .members { display: flex; flex-direction: column; gap: 4px; max-height: 180px; overflow: auto; }
</style>
