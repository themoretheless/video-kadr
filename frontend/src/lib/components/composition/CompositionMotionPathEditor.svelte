<script lang="ts">
  import CompositionPositionConversion from './CompositionPositionConversion.svelte'
  import BezierControlEditor from './BezierControlEditor.svelte'
  import { resolvedKeyframeTangents,segmentInterpolation } from '$lib/composition/tangents'
  import { EASING_PRESETS,easingPresetId } from '$lib/composition/easingPresets'
  import { sampleAnimatableValue } from '$lib/composition/keyframes'
  import { clipDurationTicks,type VisualClip } from '$lib/composition/types'
  import { configureMotionPathProgress,arriveAtMotionPathNode,editCompositionMotionPath,setMotionPathProgressKey,switchStaticPositionMode,type MotionPathEdit } from '$lib/composition/motionPathCommands'
  import { evaluateMotionPath } from '$lib/api'
  import { evaluateAgentBatch,type AgentOperation } from '$lib/agent/operations'
  import { compositionState,commitDocument } from '$lib/state/composition/core.svelte'
  let { clip }: { clip: VisualClip } = $props()
  const outsideClip = $derived(compositionState.transport.playheadTicks < clip.timelineStartTicks || compositionState.transport.playheadTicks > clip.timelineStartTicks+clipDurationTicks(clip))
  let curveDocument: typeof compositionState.document | null = null
  function commitCurve(interpolation: import('$lib/composition/types').CompositionBezierInterpolation) {
    if (curveDocument && compositionState.document !== curveDocument) compositionState.ui.message = 'Документ изменился во время редактирования кривой'
    else configure({ interpolation })
    curveDocument = null
  }
  const locked = $derived(Boolean(clip.locked || compositionState.document.tracks.some(track => track.locked && track.clips.some(candidate => candidate.id === clip.id))))
  let translateX = $state(0)
  let translateY = $state(0)
  let busy = $state(false)
  let controller: AbortController | undefined
  $effect(() => () => controller?.abort())
  async function edit(change: MotionPathEdit) {
    if (busy) return
    const captured = compositionState.document
    const clipId = clip.id
    controller = new AbortController()
    busy = true
    try {
      const updated = await editCompositionMotionPath(captured,clipId,change,controller.signal)
      if (compositionState.document !== captured) throw new Error('Документ изменился во время редактирования пути')
      commitDocument(updated)
    } catch (error) {
      compositionState.ui.message = error instanceof Error ? error.message : String(error)
    } finally { busy = false }
  }
  async function arrive(index: number) {
    if (busy) return
    const captured = compositionState.document
    controller = new AbortController()
    busy = true
    try {
      const updated = await arriveAtMotionPathNode(captured,clip.id,index,compositionState.transport.playheadTicks,controller.signal)
      if (compositionState.document !== captured) throw new Error('Документ изменился во время редактирования пути')
      commitDocument(updated)
    } catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
    finally { busy = false }
  }
  async function appendNode() {
    if (busy || clip.position?.mode !== 'path') return
    const captured = compositionState.document
    const clipId = clip.id
    const position = JSON.parse(JSON.stringify(clip.position)) as Extract<NonNullable<VisualClip['position']>,{ mode: 'path' }>
    const last = position.path.nodes.at(-1)!
    controller = new AbortController()
    busy = true
    try {
      const result = await evaluateMotionPath({ operation: 'solve',path: { nodes: [...position.path.nodes,{ id: crypto.randomUUID(),point: { x: last.point.x+100,y: last.point.y },inHandle: { x: 0,y: 0 },outHandle: { x: 0,y: 0 },segment: 'line',tangentMode: 'corner' }] } },controller.signal)
      if (controller.signal.aborted || compositionState.document !== captured) throw new Error('Документ изменился во время редактирования пути')
      commitDocument(evaluateAgentBatch(captured,[{ op: 'update_layer',clipId,patch: { position: { ...position,path: result.path } } }]))
    } catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
    finally { busy = false }
  }
  function editKey(operation: AgentOperation) {
    try { commitDocument(evaluateAgentBatch(compositionState.document,[operation])) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function configure(options: Parameters<typeof configureMotionPathProgress>[2]) {
    try { commitDocument(configureMotionPathProgress(compositionState.document,clip.id,options)) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function editBezier(point: 'p1' | 'p2',axis: 0 | 1,value: number) {
    if (clip.position?.mode !== 'path' || clip.position.progress.mode !== 'keyframes') return
    const interpolation = clip.position.progress.track.interpolation
    if (typeof interpolation !== 'object' || interpolation.kind !== 'bezier') return
    const coordinates: [number,number] = [...interpolation[point]]
    coordinates[axis] = value
    configure({ interpolation: { ...interpolation,[point]: coordinates } })
  }
  function editElastic(parameter: 'amplitude' | 'period',value: number) {
    if (clip.position?.mode !== 'path' || clip.position.progress.mode !== 'keyframes') return
    const interpolation = clip.position.progress.track.interpolation
    if (typeof interpolation !== 'object' || interpolation.kind !== 'elastic') return
    configure({ interpolation: { ...interpolation,[parameter]: value } })
  }
  function keyProgress(value: number) {
    try { commitDocument(setMotionPathProgressKey(compositionState.document,clip.id,compositionState.transport.playheadTicks,value)) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function switchMode(mode: 'xy' | 'path') {
    try { commitDocument(switchStaticPositionMode(compositionState.document,clip.id,mode,crypto.randomUUID())) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }

</script>
<fieldset class="composition-fieldset" disabled={busy || locked}>
  <legend>Траектория движения</legend>
  <CompositionPositionConversion {clip} />
  {#if clip.position?.mode === 'path'}
    <div class="composition-form-grid">
      <label>Смещение пути X<input aria-label="Смещение пути X" type="number" bind:value={translateX} /></label>
      <label>Смещение пути Y<input aria-label="Смещение пути Y" type="number" bind:value={translateY} /></label>
      <button class="btn ghost sm" onclick={() => edit({ operation: 'translate',delta: { x: translateX,y: translateY } })}>Перенести путь</button>
    </div>
    <label>Progress<input aria-label="Progress пути" disabled={outsideClip} type="number" step="0.01" value={sampleAnimatableValue(clip.position.progress,compositionState.transport.playheadTicks-clip.timelineStartTicks,compositionState.document.timeBase)} onchange={event => keyProgress(event.currentTarget.valueAsNumber)} /></label>
    <button class="btn ghost sm" disabled={outsideClip} onclick={() => keyProgress(0)}>Начало пути в текущий момент</button>
    <button class="btn ghost sm" disabled={outsideClip} onclick={() => keyProgress(1)}>Конец пути в текущий момент</button>
    {#if clip.position.progress.mode === 'keyframes'}
      {#each clip.position.progress.track.keyframes as key,index (key.tick)}
          {@const handles = resolvedKeyframeTangents(clip.position.progress.track.keyframes,index,clip.position.progress.track.interpolation)}
        <div class="composition-form-grid">
          <label>Ключ {index+1}, секунды<input aria-label={`Время ключа progress ${index+1}`} type="number" step={1/clip.position.progress.track.timeBase} value={key.tick/clip.position.progress.track.timeBase} onchange={event => {
            if (clip.position?.mode !== 'path' || clip.position.progress.mode !== 'keyframes') return
            editKey({ op: 'move_keyframe',clipId: clip.id,envelope: 'path',property: 'progress',tick: key.tick,newTick: Math.round(event.currentTarget.valueAsNumber*clip.position.progress.track.timeBase) })
          }} /></label>
          <label>Progress<input aria-label={`Значение ключа progress ${index+1}`} type="number" step="0.01" value={key.value} onchange={event => editKey({ op: 'upsert_keyframe',clipId: clip.id,envelope: 'path',property: 'progress',tick: key.tick,value: event.currentTarget.valueAsNumber })} /></label>
          {#if index < clip.position.progress.track.keyframes.length-1}
            <label>Easing сегмента<select aria-label={`Easing ключа progress ${index+1}`} value={easingPresetId(segmentInterpolation(key,clip.position.progress.track.keyframes[index+1]!,clip.position.progress.track.interpolation)) ?? ''} onchange={event => editKey({ op: 'set_keyframe_preset',clipId: clip.id,envelope: 'path',property: 'progress',ticks: [key.tick],preset: event.currentTarget.value })}>
              <option value="" disabled>Настроенная кривая</option>
              {#each EASING_PRESETS as preset (preset.id)}<option value={preset.id}>{preset.id}</option>{/each}
            </select></label>
          {/if}
          <div class="composition-form-grid">
            <button class="btn ghost sm" aria-label={`Auto ключа progress ${index+1}`} onclick={() => editKey({ op: 'set_keyframe_tangent_mode',clipId: clip.id,envelope: 'path',property: 'progress',ticks: [key.tick],mode: 'auto' })}>Auto</button>
            <button class="btn ghost sm" aria-label={`Free ключа progress ${index+1}`} onclick={() => editKey({ op: 'set_keyframe_tangent_mode',clipId: clip.id,envelope: 'path',property: 'progress',ticks: [key.tick],mode: 'free' })}>Free</button>
            <label>Связь ручек<select aria-label={`Связь ручек ключа progress ${index+1}`} value={key.tangents?.continuity ?? 'broken'} onchange={event => editKey({ op: 'edit_keyframe_tangents',clipId: clip.id,envelope: 'path',property: 'progress',tick: key.tick,continuity: event.currentTarget.value as 'smooth' | 'broken' })}><option value="broken">Broken</option><option value="smooth">Smooth</option></select></label>
          </div>
          {#each ['incoming','outgoing'] as side (side)}
            {#each ['x','y'] as axis (axis)}
              <label>{side === 'incoming' ? 'Входящая' : 'Выходящая'} {axis.toUpperCase()}<input aria-label={`Ручка progress ${index+1} ${side} ${axis}`} type="number" min={axis === 'x' ? 0 : undefined} max={axis === 'x' ? 1 : undefined} step="0.01" value={handles[side as 'incoming' | 'outgoing'][axis as 'x' | 'y']} onchange={event => editKey({ op: 'edit_keyframe_tangents',clipId: clip.id,envelope: 'path',property: 'progress',tick: key.tick,[side]: { x: handles[side as 'incoming' | 'outgoing'].x,y: handles[side as 'incoming' | 'outgoing'].y,[axis]: event.currentTarget.valueAsNumber } })} /></label>
            {/each}
          {/each}
          <button class="btn ghost sm" aria-label={`Удалить ключ progress ${index+1}`} onclick={() => editKey({ op: 'delete_keyframe',clipId: clip.id,envelope: 'path',property: 'progress',tick: key.tick })}>Удалить ключ</button>
        </div>
      {/each}
      <label>Easing progress<select aria-label="Easing progress пути" value={easingPresetId(clip.position.progress.track.interpolation) ?? ''} onchange={event => configure({ preset: event.currentTarget.value })}>
        <option value="" disabled>Настроенная кривая</option>
        {#each EASING_PRESETS as preset (preset.id)}<option value={preset.id}>{preset.id}</option>{/each}
      </select></label>
      {#if typeof clip.position.progress.track.interpolation === 'object' && clip.position.progress.track.interpolation.kind === 'bezier'}
        <BezierControlEditor value={clip.position.progress.track.interpolation} disabled={busy || locked} onpreview={() => { curveDocument ??= compositionState.document }} oncommit={commitCurve} oncancel={() => { curveDocument = null }} />
      {/if}
      {#if typeof clip.position.progress.track.interpolation === 'object' && clip.position.progress.track.interpolation.kind === 'elastic'}
        <label>Амплитуда Elastic<input aria-label="Амплитуда Elastic progress" type="number" min="1" step="0.1" value={clip.position.progress.track.interpolation.amplitude} onchange={event => editElastic('amplitude',event.currentTarget.valueAsNumber)} /></label>
        <label>Период Elastic<input aria-label="Период Elastic progress" type="number" min="0" step="0.01" value={clip.position.progress.track.interpolation.period} onchange={event => editElastic('period',event.currentTarget.valueAsNumber)} /></label>
      {/if}
      {#if typeof clip.position.progress.track.interpolation === 'object' && clip.position.progress.track.interpolation.kind === 'bezier'}
        {#each ['p1','p2'] as point (point)}
          {#each [0,1] as axis (axis)}
            <label>Bézier {point} {axis === 0 ? 'X' : 'Y'}<input aria-label={`Bézier progress ${point} ${axis === 0 ? 'X' : 'Y'}`} type="number" min={axis === 0 ? 0 : undefined} max={axis === 0 ? 1 : undefined} step="0.01" value={clip.position.progress.track.interpolation[point as 'p1' | 'p2'][axis]} onchange={event => editBezier(point as 'p1' | 'p2',axis as 0 | 1,event.currentTarget.valueAsNumber)} /></label>
          {/each}
        {/each}
      {/if}
      {#each ['before','after'] as side (side)}
        <label>{side === 'before' ? 'До ключей progress' : 'После ключей progress'}<select aria-label={`Progress ${side}`} value={clip.position.progress.track.extrapolation?.[side as 'before' | 'after'] ?? 'hold'} onchange={event => configure({ side: side as 'before' | 'after',extrapolation: event.currentTarget.value as 'hold' | 'continue' | 'loop' | 'ping_pong' | 'offset' })}>
          {#each ['hold','continue','loop','ping_pong','offset'] as mode (mode)}<option value={mode}>{mode}</option>{/each}
        </select></label>
      {/each}
    {/if}
    {#if clip.position.path.nodes.length === 1}<button class="btn ghost sm" onclick={() => switchMode('xy')}>Вернуть XY</button>{/if}
    <button class="btn ghost sm" disabled={clip.position.path.nodes.length >= 128} onclick={appendNode}>Добавить конечный узел</button>
    {#each clip.position.path.nodes as node,index (node.id)}
      <div class="composition-form-grid">
        <button class="btn ghost sm" disabled={outsideClip} onclick={() => arrive(index)}>Прибыть к узлу {index+1} сейчас</button>
        <button class="btn ghost sm" disabled={clip.position.path.nodes.length < 2} onclick={() => edit({ operation: 'remove',index })}>Удалить узел {index+1}</button>
        {#if index < clip.position.path.nodes.length-1}<button class="btn ghost sm" onclick={() => edit({ operation: 'toggle_segment',index })}>{node.segment === 'line' ? 'Сделать кривой' : 'Сделать прямой'}</button>{/if}
        <label>Узел {index+1} X<input aria-label={`Узел пути ${index+1} X`} type="number" value={node.point.x} onchange={event => edit({ operation: 'edit',index,part: 'point',value: { x: event.currentTarget.valueAsNumber,y: node.point.y } })} /></label>
        <label>Y<input aria-label={`Узел пути ${index+1} Y`} type="number" value={node.point.y} onchange={event => edit({ operation: 'edit',index,part: 'point',value: { x: node.point.x,y: event.currentTarget.valueAsNumber } })} /></label>
        <label>Ручки<select aria-label={`Режим узла пути ${index+1}`} value={node.tangentMode} onchange={event => edit({ operation: 'mode',index,mode: event.currentTarget.value as 'corner' | 'smooth' | 'auto' })}><option value="corner">Corner</option><option value="smooth">Smooth</option><option value="auto">Auto</option></select></label>
        {#each [{ field: 'inHandle',part: 'in_handle',label: 'Входящая' },{ field: 'outHandle',part: 'out_handle',label: 'Выходящая' }] as handle (handle.part)}
          {@const point = node[handle.field as 'inHandle' | 'outHandle']}
          <label>{handle.label} X<input aria-label={`${handle.label} ручка узла ${index+1} X`} type="number" value={point.x} onchange={event => edit({ operation: 'edit',index,part: handle.part as 'in_handle' | 'out_handle',value: { x: event.currentTarget.valueAsNumber,y: point.y } })} /></label>
          <label>{handle.label} Y<input aria-label={`${handle.label} ручка узла ${index+1} Y`} type="number" value={point.y} onchange={event => edit({ operation: 'edit',index,part: handle.part as 'in_handle' | 'out_handle',value: { x: point.x,y: event.currentTarget.valueAsNumber } })} /></label>
        {/each}
        {#if index < clip.position.path.nodes.length-1}<button class="btn ghost sm" onclick={() => edit({ operation: 'insert',segment: index,t: 0.5,id: crypto.randomUUID() })}>Вставить узел после {index+1}</button>{/if}
      </div>
    {/each}
  {:else}
    <button class="btn ghost sm" onclick={() => switchMode('path')}>Создать траекторию</button>
  {/if}
</fieldset>
