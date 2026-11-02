import type { NativeNestedProject } from '$lib/api'
import { expandNativeClipLinks } from './nativeClipLinks'
import { MAX_COMPOSITION_SOURCES, type CompositionSource, type WireCompositionTrack } from './types'
import { isStableId } from './validation'
/** Replace a selected media group without changing its authored clocks, look or other source uses. */
export function replaceNativeMedia(project: NativeNestedProject, compositionId: string, ids: readonly string[], source: CompositionSource, keepOriginal = false, sourceStartUs?: number): NativeNestedProject {
  const selected = expandNativeClipLinks(project, compositionId, ids)
  let sourceDelta = 0
  if (sourceStartUs !== undefined) {
    if (!Number.isSafeInteger(sourceStartUs) || sourceStartUs < 0) throw new Error('Некорректное начало исходного диапазона')
    const seed = project.compositions[compositionId]!.content.tracks.flatMap<WireCompositionTrack['clips'][number]>(track => track.clips).find(clip => clip.id === ids[0])
    if (!seed || !('placement' in seed)) throw new Error('У выбранного клипа нет исходного времени')
    sourceDelta = sourceStartUs - seed.placement.sourceInTick
  }
  return applyReplacement(project, compositionId, new Map(selected.map(id => [id, source])), keepOriginal, false, sourceDelta)
}
export function setNativeMediaVersion(project: NativeNestedProject, compositionId: string, ids: readonly string[], copy: boolean): NativeNestedProject {
  const selected = expandNativeClipLinks(project, compositionId, ids)
  const node = project.compositions[compositionId]!
  const targets = new Map<string, CompositionSource>()
  for (const id of selected) {
    const pair = node.content.metadata?.mediaVersions?.[id]
    if (!pair) throw new Error('Не у всех выбранных клипов сохранена обработанная копия')
    const sourceId = copy ? pair.copySourceId : pair.originalSourceId
    if (!Object.hasOwn(node.content.sources, sourceId)) throw new Error('Сохранённый источник не найден')
    targets.set(id, node.content.sources[sourceId]!)
  }
  return applyReplacement(project, compositionId, targets, false, true)
}
function applyReplacement(project: NativeNestedProject, compositionId: string, targets: ReadonlyMap<string, CompositionSource>, keepOriginal: boolean, preservePairs: boolean, sourceDelta = 0): NativeNestedProject {
  if (!targets.size) return project
  const selected = new Set(targets.keys())
  const node = project.compositions[compositionId]!
  const sources = { ...node.content.sources }
  const wires = new Map<string, CompositionSource>()
  for (const [id, source] of targets) {
    if (!isStableId(source.id)) throw new Error('Некорректный идентификатор источника')
    const wire: CompositionSource = { id: source.id, kind: source.kind, durationTicks: source.durationTicks, width: source.width, height: source.height, hasAudio: source.hasAudio, ...(source.audioChannels !== undefined ? { audioChannels: source.audioChannels } : {}) }
    const existing = Object.hasOwn(sources, source.id) ? sources[source.id] : undefined
    if (existing && (['id','kind','durationTicks','width','height','hasAudio','audioChannels'] as const).some(key => existing[key] !== wire[key])) throw new Error('Метаданные источника отличаются от зарегистрированных')
    if (!existing && Object.keys(sources).length >= MAX_COMPOSITION_SOURCES) throw new Error('Достигнут предел источников')
    sources[wire.id] = existing ?? wire
    wires.set(id, wire)
  }
  const versions = { ...node.content.metadata?.mediaVersions }
  let versionsChanged = false
  let changed = false
  const tracks = node.content.tracks.map(track => {
    const clips = track.clips.map(clip => {
      if (!selected.has(clip.id)) return clip
      const wire = wires.get(clip.id)!
      if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
      if (!('sourceId' in clip) || !['video','audio','image'].includes(track.kind)) throw new Error('У выбранного клипа нет заменяемого исходного медиа')
      if ((track.kind === 'video' && wire.kind !== 'video') || (track.kind === 'image' && wire.kind !== 'image') || (track.kind === 'audio' && (wire.kind === 'image' || !wire.hasAudio))) throw new Error('Тип нового источника несовместим с выбранными клипами')
      if ('sourceAudioEnabled' in clip && clip.sourceAudioEnabled && clip.placement.speedRamp?.audioPolicy !== 'mute' && !wire.hasAudio) throw new Error('Видео использует звук: выберите файл с аудиодорожкой')
      let placement = 'placement' in clip ? clip.placement : undefined
      if (placement && sourceDelta !== 0) {
        placement = { ...placement, sourceInTick: placement.sourceInTick + sourceDelta, sourceOutTick: placement.sourceOutTick + sourceDelta }
        if (![placement.sourceInTick, placement.sourceOutTick].every(Number.isSafeInteger) || placement.sourceInTick < 0 || placement.sourceOutTick <= placement.sourceInTick) throw new Error('Исходный диапазон выходит за допустимые границы')
      }
      if (placement) {
        const p = placement
        const headTime = Math.max(p.sourceHeadTimelineTick ?? 0, -(p.sourceTimelineOffsetTick ?? 0), 0)
        const head = Math.ceil(headTime * p.speed)
        const reverse = 'playbackMode' in clip ? clip.playbackMode.mode === 'reverse' : 'reversed' in clip && clip.reversed
        if (!Number.isSafeInteger(head) || p.sourceOutTick > wire.durationTicks || (reverse ? p.sourceOutTick + head > wire.durationTicks : p.sourceInTick < head)) throw new Error('Новый источник короче используемого диапазона')
      }
      if (keepOriginal) {
        const original = versions[clip.id]?.originalSourceId ?? clip.sourceId
        if (versions[clip.id]?.originalSourceId !== original || versions[clip.id]?.copySourceId !== wire.id) {
          versions[clip.id] = { originalSourceId: original, copySourceId: wire.id }; versionsChanged = true
        }
      } else if (!preservePairs && Object.hasOwn(versions, clip.id)) { delete versions[clip.id]; versionsChanged = true }
      if (clip.sourceId === wire.id && sourceDelta === 0) return clip
      changed = true
      let replacement=clip
      if(clip.sourceId!==wire.id&&'concatCutout' in clip){const {concatCutout:_cutout,...unbound}=clip;replacement=unbound}
      if ('playbackMode' in clip && clip.playbackMode.mode === 'freeze' && sourceDelta !== 0) {
        const sourceTick = clip.playbackMode.sourceTick + sourceDelta
        if (!Number.isSafeInteger(sourceTick)) throw new Error('Время стоп-кадра выходит за допустимый диапазон')
        return { ...replacement, sourceId: wire.id, placement: placement!, playbackMode: { ...clip.playbackMode, sourceTick } }
      }
      return { ...replacement, sourceId: wire.id, ...(placement ? { placement } : {}) }
    })
    return clips.some((clip,index) => clip !== track.clips[index]) ? { ...track, clips } as WireCompositionTrack : track
  })
  if (!changed && !versionsChanged) return project
  const metadata = versionsChanged ? { ...node.content.metadata, mediaVersions: versions } : node.content.metadata
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, content: { ...node.content, sources, tracks, ...(metadata ? { metadata } : {}) } } } }
}
