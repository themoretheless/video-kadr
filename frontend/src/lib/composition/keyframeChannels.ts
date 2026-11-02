import { keyframeLocalTime } from './keyframeClock'
import { clipDurationTicks, COMPOSITION_TIME_BASE, type CompositionAnimatableValue, type CompositionTrack } from './types'
import type { CompositionAutomationProperty, CompositionAutomationTarget } from '../state/composition.svelte'

export interface KeyframeChannel { id: string; target: CompositionAutomationTarget; property: CompositionAutomationProperty; value: Extract<CompositionAnimatableValue, { mode: 'keyframes' }>; start: number; duration: number; locked: boolean }
export function keyframeChannels(track: CompositionTrack): KeyframeChannel[] {
  const channels: KeyframeChannel[] = []
  for (const clip of track.clips) {
    const append = (target: CompositionAutomationTarget, property: string, value: CompositionAnimatableValue | undefined) => {
      if (target.kind === 'visual' && clip.kind !== 'audio' && clip.scaleLinked && property === 'scaleY') return
      if (value?.mode !== 'keyframes' || !value.track.keyframes.length) return
      channels.push({ id: JSON.stringify([target, property]), target, property: property as CompositionAutomationProperty, value, start: clip.timelineStartTicks, duration: clipDurationTicks(clip), locked: track.locked || !!clip.locked })
    }
    if (clip.kind !== 'audio') for (const [property, value] of Object.entries(clip.animation ?? {})) append({ kind: 'visual', clipId: clip.id }, property, value)
    if (clip.kind === 'video' || clip.kind === 'audio') for (const [property, value] of Object.entries(clip.audioAnimation ?? {})) append({ kind: 'audio', clipId: clip.id }, property, value)
    if (clip.kind === 'video') for (const mask of clip.masks ?? []) for (const property of ['x', 'y', 'width', 'height', 'rotationDegrees'] as const) append({ kind: 'mask', clipId: clip.id, maskId: mask.id }, property, mask[property])
  }
  return channels
}
export function keyframeCompositionTick(channel: KeyframeChannel, tick: number): number { return channel.start + Math.round(keyframeLocalTime(channel.value.track, tick, COMPOSITION_TIME_BASE)) }
