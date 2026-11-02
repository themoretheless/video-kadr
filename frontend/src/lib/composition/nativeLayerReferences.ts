import type { WireCompositionTrack, WireVideoEffect } from './types'
// Matches the native compositor's ordinal layer namespace; text/audio are separate.
export function nativeEffectLayers(tracks: readonly WireCompositionTrack[]): readonly WireCompositionTrack[] {
  return tracks.filter(track => ['video', 'image', 'color', 'graphic'].includes(track.kind))
}
export function rebindNativeLayerReferences(before: readonly WireCompositionTrack[], after: readonly WireCompositionTrack[]): readonly WireCompositionTrack[] {
  const oldLayers = nativeEffectLayers(before), newLayers = nativeEffectLayers(after)
  return after.map(track => {
    let changed = false
    const clips = track.clips.map(clip => {
      if (!('effects' in clip) || !clip.effects?.length) return clip
      const effects = clip.effects.map(effect => {
        const field = effect.kind === 'track_matte' ? 'matte' : effect.kind === 'gradient_wipe' || effect.kind === 'compound_blur' ? 'layer' : null
        if (!field || !('parameters' in effect)) return effect
        const parameters = effect.parameters as unknown as Record<string, unknown>
        const raw = parameters[field]
        if (raw === undefined || raw === 0) return effect
        if (typeof raw !== 'number' || !Number.isInteger(raw) || raw < 0) throw new Error('Некорректная ссылка эффекта на слой')
        const source = oldLayers[raw - 1]
        // An absent source keeps its absent-source fallback after deletion.
        if (!source) {
          if (newLayers[raw - 1]) throw new Error('Новая дорожка меняет отсутствовавшую ссылку существующего эффекта')
          return effect
        }
        const index = newLayers.findIndex(layer => layer.id === source.id)
        if (index < 0) throw new Error(`Дорожка ${source.name} используется эффектом клипа ${clip.id}`)
        if (index + 1 === raw) return effect
        if (track.locked) throw new Error('Перепривязка затрагивает заблокированную дорожку')
        changed = true
        return { ...effect, parameters: { ...parameters, [field]: index + 1 } } as WireVideoEffect
      })
      return effects.some((effect, index) => effect !== clip.effects![index]) ? { ...clip, effects } : clip
    })
    return changed ? { ...track, clips } as WireCompositionTrack : track
  })
}
