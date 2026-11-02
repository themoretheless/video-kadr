import type { NativeNestedProject } from '../api'
import type { Composition, CompositionAudioRole } from '../composition/types'
import { audioRoleMix } from '../composition/audioRoles'
import { buildCompositionWireDocument } from '../composition/payload'
export function needsNativeAudioPreview(document: Composition): boolean {
  const solo = document.tracks.some(track => track.kind === 'audio' && !track.muted && track.solo)
  return document.tracks.some(track => {
    if(track.kind==='audio')return !track.muted&&(!solo||track.solo)&&track.clips.some(clip=>clip.enabled!==false&&(clip.audioStream!==undefined||!!clip.concatGainKeys?.length||!!clip.dspEffects?.some(effect=>!effect.bypass)))
    if(track.kind==='video')return !track.muted&&track.clips.some(clip=>clip.enabled!==false&&clip.sourceAudioEnabled&&document.sources[clip.sourceId]?.hasAudio&&clip.playbackMode?.mode!=='freeze'&&(clip.independentAudio||clip.audioStream!==undefined||!!clip.concatGainKeys?.length||!!clip.dspEffects?.some(effect=>!effect.bypass)||!!clip.audioFadeInTicks||!!clip.audioFadeOutTicks))
    return false
  })
}
export function buildNativeAudioPreviewProject(document: Composition,
  gains: Partial<Record<CompositionAudioRole, number>>, durationUs: number): NativeNestedProject {
  const audioRoles = { ...document.audioRoles }
  for (const role of Object.keys(gains) as CompositionAudioRole[]) {
    const gainDb = gains[role]
    if (gainDb !== undefined) audioRoles[role] = { ...audioRoleMix(document, role), gainDb }
  }
  return { schemaVersion: 1, root: 'editor', compositions: { editor: {
    durationUs, content: buildCompositionWireDocument({ ...document, audioRoles }), instances: [],
  } } }
}
