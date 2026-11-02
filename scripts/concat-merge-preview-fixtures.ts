import { pasteNativeCaptionTracks } from '../frontend/src/lib/composition/nativeCaptionClipboard'
import { captureNativeClipboard, prepareNativeClipboardPaste } from '../frontend/src/lib/composition/nativeClipboard'
import { prepareNativeClipCopy } from '../frontend/src/lib/composition/nativeClipCopy'
import { duplicateNativeClips } from '../frontend/src/lib/composition/nativeClipDuplicate'
import { nativeClipStart } from '../frontend/src/lib/composition/nativeClipMove'
import { expandNativeClipLinks } from '../frontend/src/lib/composition/nativeClipLinks'
import { replaceNativeMedia, setNativeMediaVersion } from '../frontend/src/lib/composition/nativeMediaReplace'
import { splitNativeClips } from '../frontend/src/lib/composition/nativeClipSplit'
import { trimNativeClipEnds, trimNativeClipStarts } from '../frontend/src/lib/composition/nativeClipTrim'
import { nativeClipEnd } from '../frontend/src/lib/composition/nativeClipInsert'
import { speedRampTimelineTickAtSourceProgress } from '../frontend/src/lib/composition/speedRamp'
import { insertNativeMedia, insertNativeMediaAtFirstFree } from '../frontend/src/lib/composition/nativeClipInsert'
import { removeNativeClips } from '../frontend/src/lib/composition/nativeClipDelete'
import { editNativeTrack, type NativeTrackKind } from '../frontend/src/lib/composition/nativeTrackEdit'
import { addTimeline, removeTimeline, setTimelineVideo } from '../frontend/src/lib/composition/timelineOrder'
import { replaceClipMedia, setClipMediaVersion } from '../frontend/src/lib/composition/replaceMedia'
import { insertFreezeFrame } from '../frontend/src/lib/composition/freezeInsert'
import { addClip, addTrack, createComposition, registerSource, splitClip } from '../frontend/src/lib/composition/commands'
import { pairImportedVideoAudio } from '../frontend/src/lib/composition/importPair'
import { mergeClips } from '../frontend/src/lib/composition/mergeClips'
import { buildCompositionWireDocument } from '../frontend/src/lib/composition/payload'
import type { CompositionTransition, VideoClip } from '../frontend/src/lib/composition/types'
function source() {
  let d = registerSource(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }), { id: 'source', kind: 'video', durationTicks: 10_000_000, width: 4, height: 4, hasAudio: true })
  d = registerSource(d, { id: 'next-source', kind: 'video', durationTicks: 10_000_000, width: 4, height: 4, hasAudio: true })
  d = addTrack(d, { id: 'video', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false, clips: [] })
  const clip: VideoClip = { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 8_000_000, speed: 2, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 0.7, animation: { opacity: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'ease_in_out', keyframes: [{ tick: 0, value: 0 }, { tick: 4_000_000, value: 1 }] } } } }
  d = addClip(d, 'video', clip)
  d = addClip(d, 'video', { ...clip, id: 'next', sourceId: 'next-source', timelineStartTicks: 4_000_000, sourceInTicks: 2_000_000, sourceOutTicks: 6_000_000, speed: 1, sourceAudioEnabled: false, animation: undefined })
  return pairImportedVideoAudio(d, 'picture', { audioTrackId: 'audio', audioClipId: 'sound', linkId: 'pair' })
}
const windows: Record<string, CompositionTransition> = {
  implicit: { id: 'outer', fromClipId: 'picture', toClipId: 'next', durationTicks: 1_000_000, kind: 'dissolve' },
  explicit: { id: 'outer', fromClipId: 'picture', toClipId: 'next', durationTicks: 0, kind: 'wipe_left', timeline: { startTick: '889056000000', durationTick: '254016000000' } },
  incoming: { id: 'outer', fromClipId: null, toClipId: 'picture', durationTicks: 0, kind: 'dissolve', timeline: { startTick: '0', durationTick: '254016000000' } },
  outgoing: { id: 'outer', fromClipId: 'picture', toClipId: null, durationTicks: 0, kind: 'dissolve', timeline: { startTick: '889056000000', durationTick: '127008000000' } },
}
const holds = [false, true].map(reverse => {
  const d = source()
  const original = { ...d, tracks: d.tracks.map(t => t.kind === 'video' ? { ...t, clips: t.clips.map(c => c.id === 'picture' ? { ...c, animation: undefined, playbackMode: { mode: reverse ? 'reverse' as const : 'forward' as const } } : c) } : t) }
  let id = 0
  const inserted = insertFreezeFrame(original, 'picture', 1_500_000, 1_000_000, () => `hold-${id++}`).document
  return { reverse, original: buildCompositionWireDocument(original), inserted: buildCompositionWireDocument(inserted), times: [0, 1_499_999, 1_500_000, 1_750_000, 2_499_999, 2_500_000, 2_500_001, 4_999_999, 5_000_000, 8_999_999] }
})
const originalVersion = source()
const processedVersion = replaceClipMedia(originalVersion, 'picture', 'next-source', undefined, true)
const offVersion = setClipMediaVersion(processedVersion, 'picture', false)
const onVersion = setClipMediaVersion(offVersion, 'picture', true)
const copyReference = { ...originalVersion, tracks: originalVersion.tracks.map(t => t.kind === 'video' ? { ...t, clips: t.clips.map(c => c.id === 'picture' ? { ...c, sourceId: 'next-source' } : c) } : t) }
const versions = { original: buildCompositionWireDocument(originalVersion), processed: buildCompositionWireDocument(processedVersion), off: buildCompositionWireDocument(offVersion), on: buildCompositionWireDocument(onVersion), reference: buildCompositionWireDocument(copyReference), times: [250_000, 750_000, 1_500_000, 2_000_000, 3_999_999, 4_250_000] }
const timelineContent = buildCompositionWireDocument(source())
const timelineProject: import('../frontend/src/lib/api').NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { durationUs: 8_000_000, content: { ...timelineContent, canvas: { ...timelineContent.canvas, fpsMilli: 29970, fpsRate: { numerator: 30000, denominator: 1001 } } }, instances: [] } } }
const createdTimeline = addTimeline(timelineProject, 'root', 'new-timeline')
const removedRoot = removeTimeline(createdTimeline, 'root').document
const placed = (id: string, composition: string): import('../frontend/src/lib/api').NativeCompositionInstance => ({ id, timing: { composition, timelineStartUs: 0, sourceInUs: 0, sourceOutUs: 1_000_000 }, transform: { x: { mode: 'constant', value: 0 }, y: { mode: 'constant', value: 0 }, scaleX: { mode: 'constant', value: 1 }, scaleY: { mode: 'constant', value: 1 }, rotationDegrees: { mode: 'constant', value: 0 }, anchorX: 0.5, anchorY: 0.5 }, opacity: { mode: 'constant', value: 1 }, blendMode: 'normal' })
const linkedProject: import('../frontend/src/lib/api').NativeNestedProject = { ...createdTimeline, timelineOrder: ['root', 'new-timeline', 'parent', 'target'], compositions: { ...createdTimeline.compositions, root: { ...createdTimeline.compositions.root!, instances: [placed('outer', 'parent')] }, parent: { content: createdTimeline.compositions['new-timeline']!.content, durationUs: 1_000_000, instances: [placed('inner', 'target')] }, target: { content: createdTimeline.compositions['new-timeline']!.content, durationUs: 1_000_000, instances: [] } } }
const removedDependent = removeTimeline(linkedProject, 'target', true)
const removedTimelines = { root: removedRoot, dependent: removedDependent.document, invalidatedIds: removedDependent.invalidatedIds }
const videoSettings = { original: createdTimeline, resized: setTimelineVideo(createdTimeline, 'root', { width: 32, height: 24, numerator: 60000, denominator: 1001 }) }
const nativeTracks = (['video', 'audio', 'image', 'text', 'color', 'graphic'] as NativeTrackKind[]).map(trackKind => {
  let added = editNativeTrack(createdTimeline, 'new-timeline', { kind: 'add', id: 'manual-lane', trackKind, name: 'Manual lane' })
  added = editNativeTrack(added, 'new-timeline', { kind: 'rename', id: 'manual-lane', name: 'Renamed lane' })
  for (const flag of ['hidden', 'muted', 'solo', 'locked'] as const) if (flag in added.compositions['new-timeline']!.content.tracks.at(-1)!) added = editNativeTrack(added, 'new-timeline', { kind: 'flag', id: 'manual-lane', flag, value: true })
  const unlocked = editNativeTrack(added, 'new-timeline', { kind: 'flag', id: 'manual-lane', flag: 'locked', value: false })
  return { trackKind, added, removed: editNativeTrack(unlocked, 'new-timeline', { kind: 'remove', id: 'manual-lane', removeClips: false }) }
})
const nativeRebinds = (['track_matte', 'gradient_wipe', 'compound_blur'] as const).map(kind => {
  const transform = timelineContent.tracks.flatMap(track => track.kind === 'video' ? track.clips : [])[0]!.transform
  const effect = kind === 'track_matte' ? { kind, parameters: { matte: 3, composite: 0, reverse: false } } : kind === 'gradient_wipe' ? { kind, parameters: { layer: 3, completion: 45, softness: 20, placement: 0, invert: false } } : { kind, parameters: { layer: 3, max: 6, stretch: true, invert: false } }
  const before: import('../frontend/src/lib/api').NativeNestedProject = { ...createdTimeline, compositions: { ...createdTimeline.compositions, root: { durationUs: 8000000, instances: [], content: { ...timelineContent, metadata: undefined, sources: { still: { id: 'still', kind: 'image', durationTicks: 0, width: 4, height: 4, hasAudio: false } }, tracks: [
    { kind: 'image', id: 'empty', name: 'Empty', hidden: false, locked: false, clips: [] },
    { kind: 'image', id: 'dependent', name: 'Dependent', hidden: false, locked: false, clips: [{ id: 'picture', sourceId: 'still', timelineStartTick: 0, durationTicks: 2000000, transform, opacity: { mode: 'constant', value: 1 }, blendMode: 'normal', enabled: true, effects: [effect] }] },
    { kind: 'color', id: 'control', name: 'Control', hidden: true, locked: false, clips: [{ id: 'control-clip', timelineStartTick: 0, timelineEndTick: 2000000, width: 16, height: 16, color: { r: 127, g: 180, b: 45, a: 128 }, transform, opacity: { mode: 'constant', value: 1 }, enabled: true }] },
  ] } } } }
  return { kind, before, after: editNativeTrack(before, 'root', { kind: 'remove', id: 'empty', removeClips: false }) }
})
const nativeDeletions = Object.entries(windows).map(([name, transition]) => {
  const d = source()
  const wire = buildCompositionWireDocument({ ...d, tracks: d.tracks.map(track => track.kind === 'video' ? { ...track, transitions: [transition] } : track) })
  const before: import('../frontend/src/lib/api').NativeNestedProject = { ...createdTimeline, compositions: { ...createdTimeline.compositions, root: { durationUs: 8000000, instances: [], content: { ...wire, metadata: { ...wire.metadata, clipNames: { picture: 'Picture', next: 'Next' }, mediaVersions: { picture: { originalSourceId: 'source', copySourceId: 'next-source' } } } } } } }
  return { name, before, after: removeNativeClips(before, 'root', ['picture', 'sound']) }
})
const audio = createdTimeline.compositions.root!.content.tracks.find(track => track.kind === 'audio')!
if (audio.kind !== 'audio') throw new Error('Missing audio')
const audioNext = { ...audio.clips[0]!, id: 'next-sound', placement: { timelineStartTick: 4000000, sourceInTick: 6000000, sourceOutTick: 10000000, speed: 1 }, crossfadeInTicks: 1000000 }
const crossfadeBefore: import('../frontend/src/lib/api').NativeNestedProject = { ...createdTimeline, compositions: { ...createdTimeline.compositions, root: { ...createdTimeline.compositions.root!, content: { ...createdTimeline.compositions.root!.content, tracks: createdTimeline.compositions.root!.content.tracks.map(track => track.id === audio.id ? { ...audio, clips: [...audio.clips, audioNext] } : track) } } } }
const nativeDeleteCrossfade = { before: crossfadeBefore, after: removeNativeClips(crossfadeBefore, 'root', ['sound']) }
const nativeInsertions = (['video', 'audio', 'image'] as const).map(kind => {
  let project = createdTimeline
  const source = kind === 'image' ? { id: 'still', kind: 'image' as const, durationTicks: 0, width: 4, height: 4, hasAudio: false } : timelineContent.sources.source!
  const trackId = kind === 'image' ? 'insert-images' : kind === 'audio' ? 'audio-1' : 'video-1'
  if (kind === 'image') project = editNativeTrack(project, 'new-timeline', { kind: 'add', id: trackId, trackKind: 'image', name: 'Images' })
  const inserted = insertNativeMedia(project, 'new-timeline', source, { trackId, clipId: 'inserted', startUs: 2000000, sourceInUs: kind === 'image' ? 0 : 1000000, durationUs: 3000000 })
  let editor = registerSource(createComposition({ width: 16, height: 16, fps: 30, backgroundColor: '#000000' }), source)
  if (kind === 'video') {
    editor = addTrack(editor, { id: 'reference', kind, name: 'Reference', hidden: false, muted: false, locked: false, clips: [] })
    editor = addClip(editor, 'reference', { id: 'inserted', kind, sourceId: source.id, timelineStartTicks: 2000000, sourceInTicks: 1000000, sourceOutTicks: 4000000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 1 })
    // Ordinary primary-video payload uses canvas-normalized scaling; use the ordinary overlay
    // contract as the independent reference for native source-pixel transforms.
    editor = addTrack(editor, { id: 'canvas-base', kind: 'video', name: 'Transparent base', hidden: false, muted: true, locked: false, clips: [] })
    editor = addClip(editor, 'canvas-base', { id: 'canvas-anchor', kind: 'video', sourceId: source.id, timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 5000000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 0, sourceAudioEnabled: false, audioGain: 0 })
  } else if (kind === 'audio') {
    editor = addTrack(editor, { id: 'reference', kind, name: 'Reference', muted: false, solo: false, locked: false, clips: [] })
    editor = addClip(editor, 'reference', { id: 'inserted', kind, sourceId: source.id, timelineStartTicks: 2000000, sourceInTicks: 1000000, sourceOutTicks: 4000000, gain: 1, pan: 0 })
  } else {
    editor = addTrack(editor, { id: 'reference', kind, name: 'Reference', hidden: false, locked: false, clips: [] })
    editor = addClip(editor, 'reference', { id: 'inserted', kind, sourceId: source.id, timelineStartTicks: 2000000, durationTicks: 3000000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1 })
  }
  const wire = buildCompositionWireDocument(editor)
  const reference = { ...project, compositions: { ...project.compositions, 'new-timeline': { instances: [], content: { ...wire, canvas: project.compositions['new-timeline']!.content.canvas } } } }
  return { kind, inserted, reference, times: [0, 1999999, 2000000, 2250000, 4999999, 5000000] }
})
const nativeFirstFree = [false, true].map(occupied => {
  let before = createdTimeline
  const source = timelineContent.sources.source!
  if (occupied) for (const trackId of ['video-1', 'video-2', 'audio-1', 'audio-2']) before = insertNativeMedia(before, 'new-timeline', source, { trackId, clipId: `busy-${trackId}`, startUs: 2000000, sourceInUs: 0, durationUs: 3000000, videoAudio: false })
  return { occupied, before, after: insertNativeMediaAtFirstFree(before, 'new-timeline', source, { newTrackId: 'auto-lane', clipId: 'auto-first', startUs: 2000000, sourceInUs: 1000000, durationUs: 3000000 }), reference: nativeInsertions[0]!.reference, times: nativeInsertions[0]!.times }
})
const nativeTrims = [
  ...[false, true].flatMap(reverse => [false, true].flatMap(ramped => [-500000, 500000].map(delta => ({ reverse, ramped, slow: false, freeze: false, delta })))),
  ...[false, true].map(reverse => ({ reverse, ramped: false, slow: true, freeze: false, delta: 500000 })),
  { reverse: false, ramped: false, slow: false, freeze: true, delta: -500000 },
].map(spec => {
  const before = JSON.parse(JSON.stringify(nativeFirstFree[0]!.after)) as import('../frontend/src/lib/api').NativeNestedProject
  const node = before.compositions['new-timeline']!
  const content = { ...node.content, tracks: node.content.tracks.map(track => {
    if (track.kind !== 'video' && track.kind !== 'audio') return track
    return { ...track, clips: track.clips.map(clip => ({ ...clip,
      placement: { ...clip.placement, speed: spec.slow ? 0.5 : 1, ...(spec.ramped ? { speedRamp: { interpolation: 'linear' as const, audioPolicy: 'preserve_pitch' as const, points: [{ sourceProgressTick: 0, speed: 1 }, { sourceProgressTick: 3000000, speed: 2 }] } } : {}) },
      ...(track.kind === 'video' ? { playbackMode: spec.freeze ? { mode: 'freeze' as const, sourceTick: 2000000 } : { mode: spec.reverse ? 'reverse' as const : 'forward' as const }, opacity: { mode: 'keyframes' as const, track: { timeBase: 1000000, interpolation: 'ease_in_out' as const, keyframes: [{ tick: 0, value: 0.2 }, { tick: 3000000, value: 0.9 }] } } } : { reversed: spec.reverse }),
    })) } as import('../frontend/src/lib/composition/types').WireCompositionTrack
  }) }
  const original = { ...before, compositions: { ...before.compositions, 'new-timeline': { ...node, content } } }
  const after = trimNativeClipEnds(original, 'new-timeline', ['auto-first'], spec.delta)
  const clip = content.tracks.flatMap(track => track.clips).find(clip => clip.id === 'auto-first')!
  const end = nativeClipEnd(clip), limit = Math.min(end, end + spec.delta)
  const times = new Set([0, 1999999, 2000000, 2250000, limit - 1])
  if (spec.ramped) for (let progress = 100000; progress < 2900000; progress += 300000) {
    const local = speedRampTimelineTickAtSourceProgress(3000000, 1, { interpolation: 'linear', audioPolicy: 'preserve_pitch', points: [{ sourceProgressTick: 0, speed: 1 }, { sourceProgressTick: 3000000, speed: 2 }] }, progress)
    for (const offset of [-1, 0, 1]) if (2000000 + local + offset < limit) times.add(2000000 + local + offset)
  }
  return { ...spec, before: original, after, times: [...times].sort((a, b) => a - b), end: end + spec.delta }
})
const nativeSplits = nativeTrims.map((test, index) => {
  let serial = 0
  const after = splitNativeClips(test.before, 'new-timeline', ['auto-first'], 3250000, () => `split-${index}-${serial++}`)
  return { before: test.before, after, times: [...new Set([...test.times, 3249999, 3250000, 3250001])].sort((a,b) => a-b) }
})
const nativeHeads = nativeTrims.map(test => ({ before: test.before, after: trimNativeClipStarts(test.before, 'new-timeline', ['auto-first'], 750000), times: [...new Set([...test.times.filter(time => time < 2000000 || time >= 2750000),2750000,2750001,3250000])].sort((a,b) => a-b) }))
const nativeExtendedHeads = nativeTrims.flatMap((test, index) => {
  const extended = trimNativeClipStarts(test.before, 'new-timeline', ['auto-first'], -250000)
  let serial = 0
  const split = splitNativeClips(extended, 'new-timeline', ['auto-first'], 1900000, () => `head-split-${index}-${serial++}`)
  const retained = test.times.filter(time => time < 1750000 || time >= 2000000)
  return [
    { before: test.before, after: extended, times: [...new Set([...retained,2000000,2000001])].sort((a,b) => a-b) },
    { before: extended, after: split, times: [...new Set([...retained,1750000,1899999,1900000,1900001,1999999])].sort((a,b) => a-b) },
  ]
})
const nativeReplacements = [...nativeTrims.map(test => test.before), ...nativeExtendedHeads.slice(0,2).map(test => test.after)].map(before => {
  const node = before.compositions['new-timeline']!
  const source = { ...node.content.sources.source!, id: 'next-source' }
  const after = replaceNativeMedia(before, 'new-timeline', ['auto-first'], source)
  const members = new Set(node.content.metadata?.clipLinks?.['auto-first-link'] ?? ['auto-first'])
  const reference = { ...before, compositions: { ...before.compositions, 'new-timeline': { ...node, content: { ...node.content, sources: { ...node.content.sources, 'next-source': source }, tracks: node.content.tracks.map(track => ({ ...track, clips: track.clips.map(clip => members.has(clip.id) && 'sourceId' in clip ? { ...clip, sourceId: 'next-source' } : clip) })) } } } }
  return { before, after, reference, times: [0,2000000,2250000,3250000] }
})
const nativeVersions = nativeReplacements.map(test => {
  const source = test.after.compositions['new-timeline']!.content.sources['next-source']!
  const processed = replaceNativeMedia(test.before, 'new-timeline', ['auto-first'], source, true)
  return { original: setNativeMediaVersion(processed, 'new-timeline', ['auto-first'], false), copy: setNativeMediaVersion(processed, 'new-timeline', ['auto-first'], true), referenceOriginal: test.before, referenceCopy: test.reference, times: test.times }
})
const nativeSourceRebases = nativeReplacements.map(test => {
  const source = test.after.compositions['new-timeline']!.content.sources['next-source']!
  const after = replaceNativeMedia(test.before, 'new-timeline', ['auto-first'], source, false, 2000000)
  const node = test.reference.compositions['new-timeline']!
  const members = new Set(node.content.metadata?.clipLinks?.['auto-first-link'] ?? ['auto-first'])
  const reference = { ...test.reference, compositions: { ...test.reference.compositions, 'new-timeline': { ...node, content: { ...node.content, tracks: node.content.tracks.map(track => ({ ...track, clips: track.clips.map(clip => members.has(clip.id) && 'placement' in clip ? { ...clip, placement: { ...clip.placement, sourceInTick: clip.placement.sourceInTick+1000000, sourceOutTick: clip.placement.sourceOutTick+1000000 }, ...('playbackMode' in clip && clip.playbackMode.mode === 'freeze' ? { playbackMode: { ...clip.playbackMode, sourceTick: clip.playbackMode.sourceTick+1000000 } } : {}) } : clip) })) } } } }
  return { before: test.after, after, reference, times: test.times }
})
const duplicateSeeds: { before: import('../frontend/src/lib/api').NativeNestedProject; compositionId: string; ids: string[]; times: number[]; startUs?: number }[] = [
  ...nativeTrims.map(test => ({ before: test.before, compositionId: 'new-timeline', ids: ['auto-first'], times: test.times })),
  ...nativeExtendedHeads.slice(0,2).map(test => ({ before: test.after, compositionId: 'new-timeline', ids: ['auto-first'], times: test.times })),
  { before: nativeVersions[0]!.copy, compositionId: 'new-timeline', ids: ['auto-first'], times: [2000000,2250000,4999999] },
]
for (const transition of Object.values(windows)) {
  const d = source(), content = buildCompositionWireDocument({ ...d, tracks: d.tracks.map(track => track.kind === 'video' ? { ...track, transitions: [transition] } : track) })
  duplicateSeeds.push({ before: { schemaVersion: 1, root: 'root', compositions: { root: { durationUs: 8000000, instances: [], content } } }, compositionId: 'root', ids: content.tracks.flatMap(track => track.clips.map(clip => clip.id)), times: [0,250000,999999,3499999,3500000,3999999,4000000,4250000,7999999] })
}
for (const test of nativeRebinds) {
  duplicateSeeds.push({ before: test.before, compositionId: 'root', ids: ['picture','control-clip'], times: [0,250000,1000000,1999999] })
  const before = JSON.parse(JSON.stringify(test.before))
  before.compositions.root.content.tracks.find((track: { id: string }) => track.id === 'control').clips[0].colorAnimation = { keys: [{ t_us: 0, value: { r: 255, g: 255, b: 255, a: 192 }, in_: [0,0], out: [0,0], segment: { kind: 'Linear' } }, { t_us: 2000000, value: { r: 0, g: 0, b: 0, a: 64 }, in_: [0,0], out: [0,0], segment: { kind: 'Linear' } }], extrapolate: { before: 'Hold', after: 'Hold' } }
  duplicateSeeds.push({ before, compositionId: 'root', ids: ['picture','control-clip'], times: [0,250000,1000000,1999999] })
}
duplicateSeeds.push({ before: nativeInsertions.find(test => test.kind === 'image')!.inserted, compositionId: 'new-timeline', ids: ['inserted'], times: [2000000,2250000,4999999] })
for (const kind of ['text','color','graphic']) {
  const before = JSON.parse(JSON.stringify(createdTimeline)), node = before.compositions['new-timeline']
  const constant = (value: number) => ({ mode: 'constant', value })
  const transform = { x: constant(0), y: constant(0), scaleX: constant(1), scaleY: constant(1), rotationDegrees: constant(0), anchorX: 0.5, anchorY: 0.5 }
  const clip: any = { id: 'visual-copy', timelineStartTick: 1000000, timelineEndTick: 3000000, transform, opacity: { mode: 'keyframes', track: { timeBase: 1000000, interpolation: 'ease_in_out', keyframes: [{ tick: 0, value: 0.2 }, { tick: 2000000, value: 0.9 }] } }, enabled: true }
  if (kind === 'text') { clip.text = 'T'; clip.style = { fontFamily: 'Noto Sans', fontSize: 8, color: { red: 0.2, green: 0.8, blue: 0.4, alpha: 1 }, background: { red: 0.1, green: 0.1, blue: 0.3, alpha: 0.6 }, stroke: { red: 1, green: 0, blue: 0, alpha: 1 }, strokeWidth: 1, shadow: { red: 0, green: 0, blue: 1, alpha: 0.7 }, shadowX: 2, shadowY: 1 } }
  else { clip.width = 16; clip.height = 16; if (kind === 'color') clip.color = { r: 29, g: 181, b: 73, a: 221 }; else { clip.animationOffsetUs = 250000; clip.effects = []; clip.graphic = { layers: [{ uid: 7, enabled: true, content: { kind: 'shape', parameters: {} } }] } } }
  node.content.tracks.push({ kind, id: `copy-${kind}`, name: kind, hidden: false, locked: false, clips: [clip] })
  duplicateSeeds.push({ before, compositionId: 'new-timeline', ids: ['visual-copy'], times: [1000000,1250000,2000000,2999999] })
}
{
  const before = JSON.parse(JSON.stringify(nativeFirstFree[0]!.after))
  before.compositions['new-timeline'].content.tracks[1].transitions = [{ id: 'copy-enter', toClipId: 'auto-first', kind: 'dissolve', durationTicks: 0, timeline: { startTick: String(1750000n*254016n), durationTick: String(500000n*254016n) } },{ id: 'copy-leave', fromClipId: 'auto-first', kind: 'dissolve', durationTicks: 0, timeline: { startTick: String(4750000n*254016n), durationTick: String(500000n*254016n) } }]
  duplicateSeeds.push({ before, compositionId: 'new-timeline', ids: ['auto-first'], times: [1750000,1800000,1999999,2000000,2250000,4750000,4999999] })
}
duplicateSeeds.push(...duplicateSeeds.slice(0,14).map(test => ({ ...test, startUs: 8000000 })))
const nativeDuplicates = duplicateSeeds.map((test, index) => {
  let serial = 0
  const selected = new Set(expandNativeClipLinks(test.before, test.compositionId, test.ids)), node = test.before.compositions[test.compositionId]!
  const oldIds = new Set(node.content.tracks.flatMap(track => track.clips.map(clip => clip.id)))
  const originals = node.content.tracks.flatMap(track => track.clips.filter(clip => selected.has(clip.id)))
  const after = duplicateNativeClips(test.before, test.compositionId, test.ids, () => `duplicate-${index}-${serial++}`, test.startUs)
  const copies = after.compositions[test.compositionId]!.content.tracks.flatMap(track => track.clips.filter(clip => !oldIds.has(clip.id)))
  const origin = Math.min(...originals.map(nativeClipStart)), end = Math.max(...originals.map(nativeClipEnd)), delta = Math.min(...copies.map(nativeClipStart))-origin
  const reference = { ...test.before, compositions: { ...test.before.compositions, [test.compositionId]: { ...node, durationUs: after.compositions[test.compositionId]!.durationUs, content: { ...node.content, tracks: node.content.tracks.map(track => ({ ...track, clips: track.clips.map(clip => !selected.has(clip.id) ? clip : 'placement' in clip ? { ...clip, placement: { ...clip.placement, timelineStartTick: clip.placement.timelineStartTick+delta } } : 'durationTicks' in clip ? { ...clip, timelineStartTick: clip.timelineStartTick+delta } : { ...clip, timelineStartTick: clip.timelineStartTick+delta, timelineEndTick: clip.timelineEndTick+delta }), ...(track.kind === 'video' ? { transitions: track.transitions.map(transition => transition.timeline && ((transition.fromClipId && selected.has(transition.fromClipId)) || (transition.toClipId && selected.has(transition.toClipId))) ? { ...transition, timeline: { ...transition.timeline, startTick: String(BigInt(transition.timeline.startTick)+BigInt(delta)*254016n) } } : transition) } : {}) })) } } } }
  let renderedOrigin = origin, renderedEnd = end
  for (const track of node.content.tracks) if (track.kind === 'video') for (const transition of track.transitions) {
    if (!transition.timeline || !((transition.fromClipId && selected.has(transition.fromClipId)) || (transition.toClipId && selected.has(transition.toClipId)))) continue
    const tick = BigInt(transition.timeline.startTick), last = tick+BigInt(transition.timeline.durationTick), unit = 254016n
    renderedOrigin = Math.min(renderedOrigin,Number(tick<0n ? -((-tick+unit-1n)/unit) : tick/unit))
    renderedEnd = Math.max(renderedEnd,Number(last<0n ? -((-last)/unit) : (last+unit-1n)/unit))
  }
  const focusEnd = node.durationUs ?? Math.max(...node.content.tracks.flatMap(track => track.clips.map(nativeClipEnd)))
  return { before: test.before, after, reference, compositionId: test.compositionId, delta, start: origin+delta, times: [...new Set([...test.times,origin,origin+1,origin+250000,end-1,Math.min(focusEnd,renderedEnd)-1])].filter(time => time>=Math.max(0,renderedOrigin) && time<Math.min(focusEnd,renderedEnd)).sort((a,b) => a-b) }
})
const nativeCrossCopies = duplicateSeeds.slice(0,29).map((test,index) => {
  const source = test.before.compositions[test.compositionId]!
  const before = { ...test.before, ...(test.before.timelineOrder ? { timelineOrder: [...test.before.timelineOrder,'copy-destination'] } : {}), compositions: { ...test.before.compositions, 'copy-destination': { instances: [], content: { ...source.content, sources: {}, tracks: [], captionTracks: [], metadata: {} } } } }
  let serial = 0
  const plan = prepareNativeClipCopy(before,test.compositionId,'copy-destination',test.ids,8000000,{},() => `cross-${index}-${serial++}`)
  const origin = Math.min(...source.content.tracks.flatMap(track => track.clips.filter(clip => expandNativeClipLinks(before,test.compositionId,test.ids).includes(clip.id)).map(nativeClipStart)))
  return { before, ...plan, from: test.compositionId, to: 'copy-destination', delta: 8000000-origin, times: nativeDuplicates[index]!.times }
})
const clipboardSeeds: (typeof duplicateSeeds[number] & { pasteLuts?: import('../frontend/src/lib/api').NativeNestedProject['luts'] })[] = [...duplicateSeeds.slice(0,29)]
const identityCube='LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1'
const inverseCube='LUT_3D_SIZE 2\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0'
for (const mode of ['import','conflict','orphan'] as const) {
  const test=duplicateSeeds[0]!, node=test.before.compositions[test.compositionId]!
  const saved={ id:'clipboard-table',name:'Saved inverse',lut:{format:'cube' as const,intensity:100,text:inverseCube} }
  const before={...test.before,luts:mode==='orphan'?[]:[saved],compositions:{...test.before.compositions,[test.compositionId]:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>({...clip,effects:[{kind:'lumetri' as const,parameters:{inputLutLibrary:saved.id}}]}))}:track)}}}}
  clipboardSeeds.push({...test,before,pasteLuts:mode==='import'?[]:[{...saved,lut:{...saved.lut,text:mode==='conflict'?identityCube:inverseCube}}]})
}
const nativeClipboardPastes = clipboardSeeds.map((test,index) => {
  let serial=0
  const clipboard=captureNativeClipboard(test.before,test.compositionId,test.ids,()=>`capture-${index}-${serial++}`)
  const removed=removeNativeClips(test.before,test.compositionId,test.ids)
  const before=test.pasteLuts ? {...removed,luts:test.pasteLuts} : removed
  const targets=Object.fromEntries(clipboard.compositions.clipboard!.content.tracks.filter(track=>track.clips.length).map(track=>[track.id,'']))
  const plan=prepareNativeClipboardPaste(before,clipboard,test.compositionId,8000000,targets,()=>`paste-${index}-${serial++}`)
  const source=test.before.compositions[test.compositionId]!
  const origin=Math.min(...source.content.tracks.flatMap(track=>track.clips.filter(clip=>expandNativeClipLinks(test.before,test.compositionId,test.ids).includes(clip.id)).map(nativeClipStart)))
  return { before:plan.base, clipboard, reference:test.before, ...plan, from:test.compositionId, to:test.compositionId, delta:8000000-origin, times:nativeDuplicates[index<29?index:0]!.times }
})
const nativeCaptionPastes = (['Subtitle','CEA-608','CEA-708','Teletext','mixed'] as const).map((format,index)=>{
  const mixed=format==='mixed', seed=duplicateSeeds[0]!, source=seed.before.compositions[seed.compositionId]!, anchor=mixed?1000000:2000000
  const captions={id:'captions',name:'Saved captions',format:mixed?'Subtitle' as const:format,language:'ru',syncLock:false,hidden:false,locked:false,vttBlocks:['NOTE retained'],style:{font:'Inter',size:8,color:'#ffffff',background:false,margin:0,outline:0},clips:[
    {id:'cue-first',cueId:'file-1',timelineStartTick:String(BigInt(anchor)*254016n+1n),timelineEndTick:String(BigInt(anchor+1000000)*254016n+3n),text:'<b>I</b>',speaker:'A',settings:'align:center'},
    {id:'cue-second',cueId:'file-2',timelineStartTick:4000000,timelineEndTick:4500000,text:'V',speaker:'B',settings:''}
  ]}
  const reference={schemaVersion:1 as const,root:'new-timeline',compositions:{'new-timeline':{...source,durationUs:5000000,content:{...source.content,...(mixed?{}:{tracks:[],sources:{},metadata:{}}),captionTracks:[captions]}}}}
  let serial=0
  const clipboard=captureNativeClipboard(reference,'new-timeline',[...(mixed?['auto-first']:[]),'cue-first','cue-second'],()=>`caption-capture-${index}-${serial++}`)
  const removed=mixed?removeNativeClips(reference,'new-timeline',['auto-first']):reference, node=removed.compositions['new-timeline']!
  const before={...removed,compositions:{...removed.compositions,'new-timeline':{...node,content:{...node.content,captionTracks:[{...captions,clips:[]}]}}}}
  const targets=Object.fromEntries(clipboard.compositions.clipboard!.content.tracks.filter(track=>track.clips.length).map(track=>[track.id,'']))
  const plan=prepareNativeClipboardPaste(before,clipboard,'new-timeline',8000000,targets,()=>`caption-paste-${index}-${serial++}`)
  const captionsAfter=pasteNativeCaptionTracks(plan.base,'new-timeline',plan.captions,plan.captionDeltaUs,{},()=>`inserted-caption-${index}-${serial++}`).compositions['new-timeline']!
  const delta=8000000-anchor, shiftedReference=JSON.parse(JSON.stringify(reference)) as typeof reference
  const shifted=shiftedReference.compositions['new-timeline']!
  shifted.durationUs+=delta
  shifted.content.tracks=shifted.content.tracks.map(track=>({...track,clips:track.clips.map(clip=>'placement' in clip?{...clip,placement:{...clip.placement,timelineStartTick:clip.placement.timelineStartTick+delta}}:clip)})) as typeof shifted.content.tracks
  shifted.content.captionTracks=shifted.content.captionTracks.map(track=>({...track,clips:track.clips.map(cue=>({...cue,timelineStartTick:typeof cue.timelineStartTick==='number'?cue.timelineStartTick+delta:String(BigInt(cue.timelineStartTick)+BigInt(delta)*254016n),timelineEndTick:typeof cue.timelineEndTick==='number'?cue.timelineEndTick+delta:String(BigInt(cue.timelineEndTick)+BigInt(delta)*254016n)}))}))
  return {before:plan.base,reference,shiftedReference,clipboard,...plan,from:'new-timeline',to:'new-timeline',delta,times:[anchor,anchor+1,anchor+250000,anchor+999999,anchor+1000000,anchor+1000001,4250000,mixed?4500000:4499999],captionTracksAfter:captionsAfter.content.captionTracks,captionDurationUs:captionsAfter.durationUs,mixed}
})
export const fixture = { version: 24, nativeCaptionPastes, nativeClipboardPastes, nativeCrossCopies, nativeDuplicates, nativeSourceRebases, nativeVersions, nativeReplacements, nativeExtendedHeads, nativeHeads, nativeSplits, nativeTrims, nativeFirstFree, nativeInsertions, nativeDeletions, nativeDeleteCrossfade, nativeRebinds, nativeTracks, videoSettings, removedTimelines, createdTimeline, versions, holds, cases: Object.entries(windows).map(([name, transition]) => {
  const d = source()
  const original = { ...d, tracks: d.tracks.map(t => t.kind === 'video' ? { ...t, transitions: [transition] } : t) }
  const split = splitClip(original, 'picture', 1_500_000, 'tail')
  const joined = mergeClips(split, ['picture', 'tail']).document
  return { name, original: buildCompositionWireDocument(original), split: buildCompositionWireDocument(split), joined: buildCompositionWireDocument(joined), times: [0, 250_000, 750_000, 1_499_999, 1_500_000, 1_500_001, 2_000_000, 3_499_999, 3_500_000, 3_750_000, 3_999_999, 4_000_000, 4_250_000, 4_500_000, 5_000_000, 7_999_999] }
}) }
