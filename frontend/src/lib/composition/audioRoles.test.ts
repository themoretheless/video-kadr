import { expect, it } from 'vitest'
import { createComposition, addTrack, addClip, registerSource } from './commands'
import { audioRoleGain, audioRoleMix } from './audioRoles'
import { buildCompositionRenderRequest } from './payload'
let document = createComposition({ width: 64, height: 64, fps: 30, backgroundColor: '#000000' })
document = registerSource(document, { id: 'tone', kind: 'audio', durationTicks: 2_000_000, width: 0, height: 0, hasAudio: true })
document = addTrack(document, { id: 'sound', kind: 'audio', name: 'Sound', locked: false, muted: false, clips: [] })
document = addClip(document, 'sound', { id: 'tone-clip', kind: 'audio', sourceId: 'tone', sourceInTicks: 0, sourceOutTicks: 2_000_000, timelineStartTicks: 0, gain: 1, role: 'music', audioAnimation: { gain: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: 'loop', after: 'offset' }, keyframes: [{ tick: 0, value: 1 }, { tick: 1000, value: 2 }] } } } })
document = registerSource(document, { id: 'picture', kind: 'video', durationTicks: 2_000_000, width: 64, height: 64, hasAudio: false })
document = addTrack(document, { id: 'picture-track', kind: 'video', name: 'Picture', locked: false, muted: false, hidden: false, clips: [] })
document = addClip(document, 'picture-track', { id: 'picture-clip', kind: 'video', sourceId: 'picture', sourceInTicks: 0, sourceOutTicks: 2_000_000, timelineStartTicks: 0, transform: { x: 0, y: 0, width: 1, height: 1, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
it('uses default dialogue and all-role solo detection with mute overriding solo', () => {
  expect(audioRoleMix(document, 'dialogue')).toEqual({ gainDb: 0, muted: false, solo: false })
  expect(audioRoleGain(document)).toBe(1)
  const mixed = { ...document, audioRoles: { music: { gainDb: 6, muted: false, solo: true } } }
  expect(audioRoleGain(mixed, 'music')).toBeCloseTo(1.995262315)
  expect(audioRoleGain(mixed, 'dialogue')).toBe(0)
  expect(audioRoleGain({ ...mixed, audioRoles: { music: { gainDb: 6, muted: true, solo: true } } }, 'music')).toBe(0)
})
it('exports role multiplier separately while retaining authored curve and original document', () => {
  const mixed = { ...document, audioRoles: { music: { gainDb: -6, muted: false, solo: false } } }
  const before = JSON.stringify(mixed)
  const wire = buildCompositionRenderRequest(mixed, { profile: { container: 'audio', codec: 'wav' }, qualityTier: 'high' }).composition.tracks[0]!
  if (wire.kind !== 'audio') throw new Error('audio required')
  expect(wire.clips[0]!.roleGain).toBeCloseTo(0.50118723)
  const gain = wire.clips[0]!.gain
  expect(gain).toMatchObject({ track: { extrapolation: { before: 'loop', after: 'offset' } } })
  if (gain.mode !== 'keyframes') throw new Error('keys required')
  expect(gain.track.keyframes[0]!.value).toBe(1)
  expect(gain.track.keyframes[1]!.value).toBe(2)
  expect(JSON.stringify(mixed)).toBe(before)
})

it('supports maximum mixer boost separately and rejects overflowing export multipliers', () => {
  const boosted = { ...document, audioRoles: { music: { gainDb: 20, muted: false, solo: false } } }
  const wire = buildCompositionRenderRequest(boosted, { profile: { container: 'audio', codec: 'wav' }, qualityTier: 'high' }).composition.tracks[0]!
  if (wire.kind !== 'audio') throw new Error('audio required')
  expect(wire.clips[0]!.roleGain).toBe(10)
  expect(wire.clips[0]!.gain).toMatchObject({ track: { keyframes: [{ value: 1 }, { value: 2 }] } })
  const overflowing = { ...boosted, audioRoles: { music: { gainDb: 10000, muted: false, solo: false } } }
  expect(() => buildCompositionRenderRequest(overflowing, { profile: { container: 'audio', codec: 'wav' }, qualityTier: 'high' })).toThrow('numeric range')
})

it('retains audio channel metadata on the render wire and rejects invalid channel counts', async () => {
  const { validateComposition } = await import('./validation')
  const mono = { ...document, sources: { ...document.sources, tone: { ...document.sources.tone!, audioChannels: 1 } } }
  const wire = buildCompositionRenderRequest(mono, { profile: { container: 'audio', codec: 'wav' }, qualityTier: 'high' }).composition
  expect(wire.sources.tone!.audioChannels).toBe(1)
  for (const audioChannels of [0, 1.5, 65]) {
    const invalid = { ...mono, sources: { ...mono.sources, tone: { ...mono.sources.tone, audioChannels } } }
    expect(validateComposition(invalid).some(issue => issue.code === 'source-audio')).toBe(true)
  }
})
