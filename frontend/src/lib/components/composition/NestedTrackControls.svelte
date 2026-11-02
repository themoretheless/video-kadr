<script lang="ts">
  import {tick} from 'svelte'
  import {nativeLaneMembers} from '$lib/composition/nativeLaneGroups'
  import {activityReveal} from '$lib/agent/activityReveal.svelte'
  import { nestedComposition, nestedHistory, editNestedTrack } from '$lib/state/nestedComposition.svelte'
  import type { NativeTrackKind, NativeTrackFlag } from '$lib/composition/nativeTrackEdit'
  let { disabled = false }: { disabled?: boolean } = $props()
  const project = $derived(nestedComposition.document)
  const active = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  const tracks = $derived(project?.compositions[active]?.content.tracks ?? [])
  let root=$state<HTMLFieldSetElement>(),revealedRequest=''
  $effect(()=>{
    const request=activityReveal.request,scope=root
    if(!request||request.mode!=='nested'||request.clipId||request.compositionId!==active||request.id===revealedRequest||!scope)return
    revealedRequest=request.id
    void tick().then(()=>{if(activityReveal.request!==request||!scope.isConnected)return;const row=[...scope.querySelectorAll<HTMLElement>('[data-native-track-id]')].find(row=>row.dataset.nativeTrackId===request.trackId);row?.scrollIntoView({block:'nearest',inline:'nearest'});row?.querySelector<HTMLInputElement>('input')?.focus({preventScroll:true})})
  })
  function hasLaneClips(id:string):boolean {
    const content=project?.compositions[active]?.content
    if(!content)return false
    try { const members=nativeLaneMembers(content,id);return tracks.some(track=>members.includes(track.id)&&track.clips.length>0) }
    catch { return tracks.some(track=>track.id===id&&track.clips.length>0) }
  }
  let kind = $state<NativeTrackKind>('video'), name = $state('Новая дорожка'), removeClips = $state(false)
  $effect(() => { if (project && active) removeClips = false })
  const labels: Record<NativeTrackFlag, string> = { hidden: 'Скрыта', muted: 'Без звука', solo: 'Соло', locked: 'Заблокирована' }
</script>
{#if project}
  <fieldset bind:this={root} class="composition-fieldset" disabled={disabled || nestedHistory.busy}>
    <legend>Дорожки таймлайна</legend>
    {#if Object.keys(project.compositions[active]?.content.metadata?.concatLaneGroups??{}).length}<p>Видео и его стоп-кадры скрываются, блокируются и удаляются вместе.</p>{/if}
    {#each tracks as track (track.id)}
      <div data-native-track-id={track.id}>
        <input aria-label={`Название дорожки ${track.id}`} value={track.name} disabled={track.locked} onchange={event => void editNestedTrack(active, { kind: 'rename', id: track.id, name: event.currentTarget.value })} />
        {#each ['hidden', 'muted', 'solo', 'locked'] as flag (flag)}
          {#if flag in track}
            <label><input type="checkbox" aria-label={`${labels[flag as NativeTrackFlag]} ${track.id}`} checked={Boolean((track as unknown as Record<string, unknown>)[flag])} onchange={event => void editNestedTrack(active, { kind: 'flag', id: track.id, flag: flag as NativeTrackFlag, value: event.currentTarget.checked })} /> {labels[flag as NativeTrackFlag]}</label>
          {/if}
        {/each}
        <button class="btn ghost sm" type="button" disabled={track.locked || (hasLaneClips(track.id) && !removeClips)} onclick={() => void editNestedTrack(active, { kind: 'remove', id: track.id, removeClips })}>Удалить дорожку {track.name}</button>
      </div>
    {/each}
    <label><input type="checkbox" bind:checked={removeClips} /> Удалять дорожку вместе с её клипами и связанными клипами</label>
    <label>Тип <select aria-label="Тип новой дорожки" bind:value={kind}>
      <option value="video">Видео</option><option value="audio">Звук</option><option value="image">Изображения</option><option value="text">Текст</option><option value="color">Заливки</option><option value="graphic">Графика</option>
    </select></label>
    <label>Название <input aria-label="Название новой дорожки" bind:value={name} /></label>
    <button class="btn ghost sm" type="button" onclick={() => void editNestedTrack(active, { kind: 'add', id: `track-${crypto.randomUUID()}`, trackKind: kind, name })}>Добавить дорожку</button>
  </fieldset>
{/if}
