<script lang="ts">
  import { untrack } from 'svelte'
  import NestedPositionConversion from './NestedPositionConversion.svelte'
  import { nestedPathOverlay, clearPathOverlay } from '$lib/state/nestedPathOverlay.svelte'
  import { nestedComposition, nestedHistory, editNestedMotionPath, nestedAgentGeneration } from '$lib/state/nestedComposition.svelte'
  import { nativeClipStart } from '$lib/composition/nativeClipMove'
  import { nativeClipEnd } from '$lib/composition/nativeClipInsert'
  import { agentSession } from '$lib/agent/session.svelte'
  import { authState } from '$lib/state/auth.svelte'
  import type { CompositionPathNode } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  let selected = $state(''), deltaX = $state(0), deltaY = $state(0), formVersion = $state(0)
  const overlayOwner=Symbol('path inspector')
  let overlayEnabled=$state(false)
  let overlayContext:{projectId:string|null;scope:string;generation:number;token:string|null}|null=null
  let alive = true
  $effect(() => () => { alive = false })
  const project = $derived(nestedComposition.document)
  const scope = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  const entries = $derived((project?.compositions[scope]?.content.tracks ?? []).flatMap(track => track.clips.map(clip => ({ track, clip }))).filter(entry => 'transform' in entry.clip))
  const current = $derived(entries.find(entry => entry.clip.id === selected) ?? entries[0])
  const transform = $derived(current && 'transform' in current.clip ? current.clip.transform : undefined)
  const position = $derived(transform?.position)
  const xy = $derived(position?.mode === 'xy' ? position : transform)
  const staticPosition = $derived(xy?.x.mode === 'constant' && xy?.y.mode === 'constant')
  const partTitle = (part: string) => part === 'point' ? 'Точка' : part === 'in_handle' ? 'Входящая ручка' : 'Исходящая ручка'
  const outside = $derived(!current || nestedComposition.timeUs < nativeClipStart(current.clip) || nestedComposition.timeUs > nativeClipEnd(current.clip))
  const blocked = $derived(disabled || nestedHistory.busy || !!agentSession.lock || !!current?.track.locked)
  function toggleOverlay(enabled:boolean){overlayContext={projectId:nestedComposition.projectId,scope,generation:nestedAgentGeneration(),token:authState.token};overlayEnabled=enabled}
  $effect(()=>{
    const generation=nestedAgentGeneration(),token=authState.token
    if(overlayEnabled&&(!overlayContext||overlayContext.projectId!==nestedComposition.projectId||overlayContext.scope!==scope||overlayContext.generation!==generation||overlayContext.token!==token))overlayEnabled=false
    if(!overlayEnabled||!project||!current||position?.mode!=='path'||!token||disabled){clearPathOverlay(overlayOwner);return}
    const previous=untrack(()=>nestedPathOverlay.value)
    if(previous?.owner!==overlayOwner||previous.base!==project||previous.clipId!==current.clip.id||previous.compositionId!==scope||previous.token!==token)nestedPathOverlay.value={owner:overlayOwner,base:project,clipId:current.clip.id,compositionId:scope,token,generation,projectId:nestedComposition.projectId}
  })
  $effect(()=>()=>clearPathOverlay(overlayOwner))
  function operation(change: Record<string, unknown>) {
    const base = project, target = current, composition = scope, session = authState.token
    if (!base || !target || blocked) return
    return editNestedMotionPath(composition, { ...change, clipId: target.clip.id }, () => {
      if (!alive || project !== base || scope !== composition || current?.clip.id !== target.clip.id || authState.token !== session) throw new Error('Контекст редактирования пути изменился')
    }).finally(() => { if (alive) formVersion++ })
  }
  function edit(change: Record<string, unknown>) { return operation({ op: 'edit_motion_path', edit: change }) }
  function coordinate(node: CompositionPathNode, index: number, part: 'point' | 'in_handle' | 'out_handle', axis: 'x' | 'y', value: number) {
    const point = part === 'point' ? node.point : part === 'in_handle' ? node.inHandle : node.outHandle
    void edit({ operation: 'edit', index, part, value: { ...point, [axis]: value } })
  }
  function append() {
    if (position?.mode !== 'path') return
    const last = position.path.nodes.at(-1)!
    void edit({ operation: 'append', id: crypto.randomUUID(), point: { x: last.point.x + 100, y: last.point.y } })
  }
</script>
{#if current}
  <details><summary>Геометрия траектории</summary>
    <label>Клип <select aria-label="Клип для геометрии пути" value={current.clip.id} disabled={disabled || nestedHistory.busy || !!agentSession.lock} onchange={event => selected = event.currentTarget.value}>{#each entries as entry (entry.clip.id)}<option value={entry.clip.id}>{entry.track.name} · {entry.clip.id}</option>{/each}</select></label>
    <fieldset disabled={blocked}>
      {#if position?.mode === 'path'}
        <label><input type="checkbox" aria-label="Редактировать путь на кадре" checked={overlayEnabled} disabled={!authState.token} onchange={event=>toggleOverlay(event.currentTarget.checked)} />Редактировать путь на кадре</label>
        <label>Смещение X <input aria-label="Смещение траектории X" type="number" bind:value={deltaX} /></label>
        <label>Смещение Y <input aria-label="Смещение траектории Y" type="number" bind:value={deltaY} /></label>
        <button type="button" class="btn ghost sm" onclick={() => edit({ operation: 'translate', delta: { x: deltaX, y: deltaY } })}>Перенести путь</button>
        <button type="button" class="btn ghost sm" onclick={() => edit({ operation: 'solve' })}>Пересчитать ручки</button>
        <button type="button" class="btn ghost sm" disabled={position.path.nodes.length >= 128} onclick={append}>Добавить конечный узел</button>
        {#if position.path.nodes.length === 1}<button type="button" class="btn ghost sm" onclick={() => operation({ op: 'switch_position_mode', positionMode: 'xy', nodeId: 'unused' })}>Вернуть XY</button>{/if}
        {#key formVersion}
        {#each position.path.nodes as node, index (node.id)}
          <div class="node">
            <strong>Узел {index + 1}</strong>
            <button type="button" class="btn ghost sm" aria-label={`Прибыть к узлу ${index + 1} сейчас`} disabled={outside} onclick={() => operation({op:'arrive_at_path_node',index,playheadTicks:nestedComposition.timeUs})}>Прибыть к узлу сейчас</button>
            {#each ['point', 'in_handle', 'out_handle'] as part (part)}
              {@const point = part === 'point' ? node.point : part === 'in_handle' ? node.inHandle : node.outHandle}
              {#each ['x', 'y'] as axis (axis)}
                <label>{partTitle(part)} {axis.toUpperCase()} <input aria-label={`Узел ${index + 1}: ${partTitle(part)} ${axis.toUpperCase()}`} type="number" step="any" value={point[axis as 'x' | 'y']} onchange={event => coordinate(node, index, part as 'point' | 'in_handle' | 'out_handle', axis as 'x' | 'y', event.currentTarget.valueAsNumber)} /></label>
              {/each}
            {/each}
            <label>Ручки <select aria-label={`Узел ${index + 1}: режим ручек`} value={node.tangentMode} onchange={event => edit({ operation: 'mode', index, mode: event.currentTarget.value })}>{#each ['corner', 'smooth', 'auto'] as mode (mode)}<option value={mode}>{mode === 'corner' ? 'Угловой' : mode === 'smooth' ? 'Гладкий' : 'Авто'}</option>{/each}</select></label>
            <button type="button" class="btn ghost sm" aria-label={`Удалить узел траектории ${index + 1}`} disabled={position.path.nodes.length < 2} onclick={() => edit({ operation: 'remove', index })}>Удалить узел</button>
            {#if index < position.path.nodes.length - 1}
              <button type="button" class="btn ghost sm" aria-label={`Переключить сегмент траектории ${index + 1}`} onclick={() => edit({ operation: 'toggle_segment', index })}>{node.segment === 'line' ? 'Сделать кривой' : 'Сделать прямой'}</button>
              <button type="button" class="btn ghost sm" aria-label={`Разделить сегмент траектории ${index + 1}`} onclick={() => edit({ operation: 'insert', segment: index, t: 0.5, id: crypto.randomUUID() })}>Вставить узел посередине</button>
            {/if}
          </div>
        {/each}
        {/key}
      {:else}
        <button type="button" class="btn ghost sm" disabled={!staticPosition} onclick={() => operation({ op: 'switch_position_mode', positionMode: 'path', nodeId: crypto.randomUUID() })}>Создать путь из статической позиции</button>
        {#if !staticPosition}<p>Для анимированных X/Y требуется преобразование движения.</p>{/if}
      {/if}
    </fieldset>
    <NestedPositionConversion compositionId={scope} clipId={current.clip.id} {disabled} />
  </details>
{/if}
<style>fieldset{border:0;padding:.5rem 0}.node{padding:.5rem 0;border-top:1px solid var(--border,#444)}label{display:flex;gap:.4rem;align-items:center;margin:.3rem 0}input{width:7rem}summary{cursor:pointer}</style>
