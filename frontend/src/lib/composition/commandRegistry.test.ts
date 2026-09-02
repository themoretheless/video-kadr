import { beforeEach, describe, expect, it } from 'vitest'
import { addMediaInfoToComposition, compositionState, newComposition, selectCompositionClip, setCompositionPlayhead } from '$lib/state/composition.svelte.js'
import { addClip, addTrack } from './commands.js'
import { compositionCommandState, executeCompositionCommand } from './commandRegistry.js'
import { COMPOSITION_TIME_BASE } from './types.js'

describe('composition command registry', () => {
  beforeEach(() => newComposition())

  it('shares availability and execution across every UI entry point', () => {
    expect(compositionCommandState('composition.split')).toEqual({ enabled: false, disabledReason: 'Сначала выберите клип' })
    addMediaInfoToComposition({ id: 'source', url: '/source.mp4', filename: 'source.mp4', mediaType: 'video', duration: 4, width: 640, height: 360 })
    compositionState.document = addTrack(compositionState.document, { id: 'track', kind: 'video', name: 'Video', clips: [], muted: false, hidden: false, locked: false })
    compositionState.document = addClip(compositionState.document, 'track', {
      id: 'clip', kind: 'video', sourceId: 'source', timelineStartTicks: 0,
      sourceInTicks: 0, sourceOutTicks: 4 * COMPOSITION_TIME_BASE,
      transform: { x: 0, y: 0, width: 640, height: 360, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1,
    })
    selectCompositionClip('track', 'clip')
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    expect(compositionCommandState('composition.split')).toEqual({ enabled: true })
    expect(executeCompositionCommand('composition.split')).toBe(true)
    expect(compositionState.document.tracks.find((track) => track.id === 'track')!.clips).toHaveLength(2)
    expect(executeCompositionCommand('composition.undo')).toBe(true)
    expect(compositionState.document.tracks.find((track) => track.id === 'track')!.clips).toHaveLength(1)
  })
})
