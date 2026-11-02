<script lang="ts">
  import BezierControlEditor from './BezierControlEditor.svelte'
  import { segmentInterpolation, type KeyframeTangentIntent } from '$lib/composition/tangents'
  import { keyframeLocalTime } from '$lib/composition/keyframeClock'
  import type { CompositionAnimatableValue, CompositionBezierInterpolation } from '$lib/composition/types'
  let { animation, label, disabled = false, onedit, onpreview }: {
    animation: Extract<CompositionAnimatableValue, { mode: 'keyframes' }>
    label: string
    disabled?: boolean
    onedit: (tick: number, intent: Extract<KeyframeTangentIntent, { kind: 'point' }>, gesture: object) => void
    onpreview?: (edit: {tick:number;intent:Extract<KeyframeTangentIntent,{kind:'point'}>} | null, gesture:object) => void
  } = $props()
  let selected = $state(''), preview = $state(false)
  const segments = $derived(animation.track.keyframes.slice(0, -1))
  const index = $derived(Math.max(0, segments.findIndex(key => String(key.tick) === selected)))
  const left = $derived(segments[index])
  const right = $derived(animation.track.keyframes[index + 1])
  const mode = $derived(left && right ? segmentInterpolation(left, right, animation.track.interpolation) : undefined)
  const bezier = $derived(typeof mode === 'object' && mode.kind === 'bezier' ? mode : undefined)
  let gesture: object | undefined
  function pointEdit(value: CompositionBezierInterpolation) {
    if (!bezier || !left || !right || disabled) return undefined
    const changed = (['p1', 'p2'] as const).filter(point => value[point].some((number, axis) => number !== bezier[point][axis]))
    if (changed.length !== 1) return undefined
    const point = changed[0]!
    return {tick:point === 'p1' ? left.tick : right.tick,intent:{kind:'point' as const,side:point === 'p1' ? 'outgoing' as const : 'incoming' as const,x:value[point][0],y:value[point][1]}}
  }
  function draft(value: CompositionBezierInterpolation) {
    gesture ??= {}
    preview = true
    onpreview?.(pointEdit(value) ?? null, gesture)
  }
  function cancel() {
    const current = gesture
    gesture = undefined; preview = false
    if(current)onpreview?.(null,current)
  }
  function commit(value: CompositionBezierInterpolation) {
    const current = gesture ?? {}
    const edit = pointEdit(value)
    gesture = undefined; preview = false
    if(edit)onedit(edit.tick,edit.intent,current)
    onpreview?.(null,current)
  }

</script>
{#if left && right}
  <details>
    <summary>График Bézier</summary>
    <label>Сегмент <select aria-label={`${label}: сегмент кривой`} value={String(left.tick)} {disabled} onchange={event => selected = event.currentTarget.value}>
      {#each segments as key, i (key.tick)}<option value={String(key.tick)}>{keyframeLocalTime(animation.track, key.tick, 1)}–{keyframeLocalTime(animation.track, animation.track.keyframes[i + 1]!.tick, 1)} с</option>{/each}
    </select></label>
    {#if bezier}
      {#key left.tick}
        <BezierControlEditor value={bezier} {disabled} onpreview={draft} oncommit={commit} oncancel={cancel} />
      {/key}
      <p>Левая ручка — исходящая, правая — входящая. Стрелки меняют координаты; Escape отменяет перетаскивание.</p>
      {#if preview}<p role="status">Предпросмотр: отпустите ручку, чтобы применить изменение.</p>{/if}
    {:else}
      <p>Выберите для начала сегмента переход Bézier из списка переходов или задайте касательную численно.</p>
    {/if}
  </details>
{/if}
<style>label{display:flex;gap:.4rem;align-items:center}select{min-width:0;max-width:100%}p{font-size:.75rem}summary{cursor:pointer}</style>
