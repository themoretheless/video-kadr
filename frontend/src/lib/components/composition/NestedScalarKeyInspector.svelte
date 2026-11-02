<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { authState } from '$lib/state/auth.svelte'
  import { nestedEffectPreview, clearEffectPreview } from '$lib/state/nestedEffectPreview.svelte'

  import NativeScalarBezierEditor from './NativeScalarBezierEditor.svelte'
  import GraphicKeyTangents from './GraphicKeyTangents.svelte'
  import type { KeyframeTangentIntent } from '$lib/composition/tangents'
  import { nestedComposition, nestedHistory, editNestedScalarKey } from '$lib/state/nestedComposition.svelte'
  import { agentSession } from '$lib/agent/session.svelte'
  import { applyNativeKeyframeOperation, readNativeKeyframes, readNativeKeyframeValue } from '$lib/agent/nativeKeyframes'
  import { keyframeAuthoredTick, keyframeLocalTime } from '$lib/composition/keyframeClock'
  import { nativeClipStart } from '$lib/composition/nativeClipMove'
  import { nativeClipEnd } from '$lib/composition/nativeClipInsert'
  import { sampleAnimatableValue } from '$lib/composition/keyframes'
  import { EASING_PRESETS } from '$lib/composition/easingPresets'
  let { disabled = false }: { disabled?: boolean } = $props()
  let selected = $state(''), channel = $state('x'), error = $state('')
  const project = $derived(nestedComposition.document)
  const scope = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  const entries = $derived((project?.compositions[scope]?.content.tracks ?? []).flatMap(track => track.clips.map(clip => ({ track, clip }))))
  const current = $derived(entries.find(entry => entry.clip.id === selected) ?? entries[0])
  const pathPosition = $derived(current && 'transform' in current.clip && current.clip.transform.position?.mode === 'path')
  const channels = $derived(current ? [...(current.track.kind === 'audio' ? [] : [...(pathPosition ? ['progress'] : ['x', 'y']), 'scaleX', 'scaleY', 'rotationDegrees', 'opacity']), ...(['audio', 'video'].includes(current.track.kind) ? ['gain', 'pan'] : [])] : [])
  const property = $derived(channels.includes(channel) ? channel : channels[0] ?? 'x')
  const envelope = $derived(property === 'progress' ? 'path' : property === 'gain' || property === 'pan' ? 'audio' : 'visual')
  const inspected = $derived.by(() => {
    if (!project || !current) return undefined
    try { return readNativeKeyframes(project, scope, { clipId: current.clip.id, property, envelope }) } catch { return undefined }
  })
  const curve = $derived.by(() => {
    if (!project || !current) return undefined
    try { return readNativeKeyframeValue(project, scope, { clipId: current.clip.id, property, envelope }) } catch { return undefined }
  })
  const track = $derived(curve?.mode === 'keyframes' ? curve.track : undefined)
  const blocked = $derived(disabled || nestedHistory.busy || !!agentSession.lock || !!current?.track.locked)
  function edit(operation: Record<string, unknown>) {
    const document = project, target = current, composition = scope, selectedProperty = property, session = authState.token
    if (!target || blocked) return
    error = ''
    return editNestedScalarKey(composition, { ...operation, clipId: target.clip.id, property, envelope }, () => {
      if (authState.token !== session) throw new Error('Сессия редактора изменилась')
      if (project !== document || scope !== composition || current?.clip.id !== target.clip.id || property !== selectedProperty) throw new Error('Выбранный канал изменился')
    })
  }
  function tangent(tick: number, intent: KeyframeTangentIntent) {
    if(intent.kind==='mode')return edit({op:'set_keyframe_tangent_mode',ticks:[tick],mode:intent.mode})
    if(intent.kind==='continuity')return edit({op:'edit_keyframe_tangents',tick,continuity:intent.continuity})
    return edit({op:'edit_keyframe_tangents',tick,[intent.side]:{x:intent.x,y:intent.y}})
  }
  const previewOwner = Symbol('native scalar curve')
  let previewAddress = $state('')
  const pending = new SvelteSet<object>()
  function graphPreview(draft:{tick:number;intent:Extract<KeyframeTangentIntent,{kind:'point'}>}|null,gesture:object){
    if(!draft){if(!pending.has(gesture))clearEffectPreview(previewOwner,gesture);return}
    const base=project,target=current,token=authState.token
    if(!base||!target||!token||blocked){clearEffectPreview(previewOwner,gesture);return}
    try{
      const candidate=applyNativeKeyframeOperation(base,scope,{op:'edit_keyframe_tangents',clipId:target.clip.id,property,envelope,tick:draft.tick,[draft.intent.side]:{x:draft.intent.x,y:draft.intent.y}})
      previewAddress=JSON.stringify([scope,target.clip.id,property])
      nestedEffectPreview.value={owner:previewOwner,gesture,base,project:candidate,timeUs:nestedComposition.timeUs,compositionId:scope,token}
    }catch{clearEffectPreview(previewOwner,gesture)}
  }
  async function graphCommit(tick:number,intent:Extract<KeyframeTangentIntent,{kind:'point'}>,gesture:object){
    pending.add(gesture)
    try{await tangent(tick,intent)}finally{pending.delete(gesture);clearEffectPreview(previewOwner,gesture)}
  }
  $effect(()=>{
    const draft=nestedEffectPreview.value
    if(draft?.owner===previewOwner&&(disabled||!!agentSession.lock||!!current?.track.locked||previewAddress!==JSON.stringify([scope,current?.clip.id,property])||draft.base!==project||draft.timeUs!==nestedComposition.timeUs||draft.compositionId!==scope||draft.token!==authState.token||(nestedHistory.busy&&!pending.has(draft.gesture))))clearEffectPreview(previewOwner)
  })
  $effect(()=>()=>clearEffectPreview(previewOwner))
  function move(tick: number, seconds: number) {
    if (!track) return
    try { edit({ op: 'move_keyframe', tick, newTick: keyframeAuthoredTick(track, seconds, 1) }) }
    catch (reason) { error = reason instanceof Error ? reason.message : String(reason) }
  }
  function add() {
    if (!current || !curve) return
    const local = nestedComposition.timeUs - nativeClipStart(current.clip)
    if (local < 0 || nestedComposition.timeUs > nativeClipEnd(current.clip)) { error = 'Курсор должен находиться внутри клипа'; return }
    const tick = track ? keyframeAuthoredTick(track, local) : Math.round(local)
    const value = sampleAnimatableValue(curve, local)
    edit({ op: 'upsert_keyframe', tick, value })
  }
</script>
{#if current}
  <details><summary>Ключи положения, прозрачности и звука</summary>
    <label>Клип <select aria-label="Клип для скалярных ключей" value={current.clip.id} disabled={disabled || nestedHistory.busy || !!agentSession.lock} onchange={event => selected = event.currentTarget.value}>{#each entries as entry (entry.clip.id)}<option value={entry.clip.id}>{entry.track.name} · {entry.clip.id}</option>{/each}</select></label>
    <label>Канал <select aria-label="Канал скалярных ключей" value={property} onchange={event => channel = event.currentTarget.value}>{#each channels as name (name)}<option value={name}>{name === 'progress' ? 'Движение по контуру' : name}</option>{/each}</select></label>
    <fieldset disabled={blocked || !inspected}>
      <button type="button" class="btn ghost sm" aria-label="Добавить скалярный ключ по курсору" onclick={add}>+ Ключ по курсору</button>
      {#if track}
        {#if curve?.mode === 'keyframes'}
          {#key project}
            {#key `${scope}/${current.clip.id}/${property}/${nestedComposition.timeUs}`}
              {#key authState.token}
                <NativeScalarBezierEditor animation={curve} label={`Кривая ${current.clip.id} ${property}`} disabled={blocked} onedit={graphCommit} onpreview={graphPreview} />
              {/key}
            {/key}
          {/key}
        {/if}
        {#each ['before', 'after'] as side (side)}
          <label>{side === 'before' ? 'До ключей' : 'После ключей'} <select aria-label={`Скалярная экстраполяция ${side}`} value={track.extrapolation?.[side as 'before' | 'after'] ?? 'hold'} onchange={event => edit({ op: 'set_keyframe_extrapolation', side, extrapolation: event.currentTarget.value })}>{#each ['hold', 'continue', 'loop', 'ping_pong', 'offset'] as mode (mode)}<option value={mode}>{mode}</option>{/each}</select></label>
        {/each}
        {#each track.keyframes as key (key.tick)}
          <div class="key">
            <label>Время, с <input aria-label={`Скалярный ключ ${key.tick}: время`} type="number" step="0.001" value={keyframeLocalTime(track, key.tick, 1)} onchange={event => move(key.tick, Number(event.currentTarget.value))} /></label>
            <label>Значение <input aria-label={`Скалярный ключ ${key.tick}: значение`} type="number" step="any" value={key.value} onchange={event => edit({ op: 'edit_keyframe', tick: key.tick, value: Number(event.currentTarget.value) })} /></label>
            <label>Переход <select aria-label={`Скалярный ключ ${key.tick}: переход`} value={inspected?.mode === 'keyframes' ? inspected.keyframes.find(item => item.tick === key.tick)?.preset_id ?? '' : ''} onchange={event => edit({ op: 'set_keyframe_preset', ticks: [key.tick], preset: event.currentTarget.value })}><option value="" disabled>Своя кривая</option>{#each EASING_PRESETS as preset (preset.id)}<option value={preset.id}>{preset.id}</option>{/each}</select></label>
            {#if curve}<GraphicKeyTangents animation={curve} tick={key.tick} label={`Скалярный ключ ${key.tick}`} onintent={intent=>tangent(key.tick,intent)} />{/if}
            <button type="button" class="btn ghost sm" aria-label={`Удалить скалярный ключ ${key.tick}`} onclick={() => edit({ op: 'delete_keyframe', tick: key.tick })}>Удалить</button>
          </div>
        {/each}
      {/if}
    </fieldset>
    {#if !inspected}<p>Этот канал недоступен для текущего типа положения.</p>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  </details>
{/if}
<style>fieldset{border:0;padding:.5rem 0}label{display:flex;gap:.4rem;align-items:center;margin-bottom:.4rem}.key{display:flex;flex-wrap:wrap;gap:.4rem;padding:.4rem 0}input{width:7rem}</style>
