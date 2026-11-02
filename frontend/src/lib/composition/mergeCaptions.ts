import { unlinkNativeClips } from './nativeClipLinks'
import { removeNativeClips } from './nativeClipDelete'
import { MAX_TEXT_LENGTH } from './types'
import { deleteClip, findClipLocation } from './commands'
import { assertCompositionLocksPreserved } from './locks'
import { assertValidComposition } from './validation'
import { clipEndTicks, type Composition, type TextClip } from './types'

/** Caption union keeps the earliest cue's identity, appearance and authored animation. */
export function mergeCaptions(document: Composition, ids: readonly string[]) {
  assertValidComposition(document)
  if (!Array.isArray(ids) || ids.length > 512 || ids.some(id => typeof id !== 'string')) throw new Error('Некорректный список субтитров')
  const unique = [...new Set(ids)]
  if (unique.length < 2) throw new Error('Выберите минимум два субтитра')
  const roles = new Set(document.metadata?.captionTextTracks ?? [])
  const locations = unique.map(id => findClipLocation(document, id))
  const lane = locations[0]!.track
  for (const { track, clip } of locations) {
    if (track.kind !== 'text' || clip.kind !== 'text' || !roles.has(track.id)) throw new Error('Выбранный клип не является субтитром')
    if (track.id !== lane.id) throw new Error('Субтитры должны находиться на одной дорожке')
    if (track.locked || clip.locked) throw new Error('Субтитр или дорожка заблокированы')
  }
  const selected = new Set(unique)
  const clips = locations.map(({clip}) => clip as TextClip).sort((a,b) => a.timelineStartTicks - b.timelineStartTicks)
  const first = clips[0]!
  const end = Math.max(...clips.map(clipEndTicks))
  if (lane.clips.some(clip => !selected.has(clip.id) && clip.timelineStartTicks < end && clipEndTicks(clip) > first.timelineStartTicks)) {
    throw new Error('Объединённый субтитр пересекается с другой репликой')
  }
  let next = document
  for (const clip of clips.slice(1)) next = deleteClip(next, clip.id)
  const merged = {...first, durationTicks: end - first.timelineStartTicks, text: clips.map(clip => clip.text).join('\n')}
  next = {...next, tracks: next.tracks.map(track => track.id !== lane.id ? track : {
    ...track, clips: track.clips.map(clip => clip.id === first.id ? merged : clip),
  }) as Composition['tracks']}
  assertValidComposition(next)
  assertCompositionLocksPreserved(document, next)
  return { document: next, trackId: lane.id, record: {
    layer_id: first.id, t_start_us: first.timelineStartTicks, t_end_us: end,
    content: merged.text, merged: clips.length, removed: clips.slice(1).map(clip => clip.id),
  } }
}

/** Native editable caption layers; keep deletion/link cleanup owned by native commands. */
export function mergeNestedCaptions(project: import('../api').NativeNestedProject, compositionId: string, ids: readonly string[]) {
  if (!Array.isArray(ids) || ids.length > 512 || ids.some(id => typeof id !== 'string')) throw new Error('Некорректный список субтитров')
  const unique = [...new Set(ids)]
  if (unique.length < 2) throw new Error('Выберите минимум два субтитра')
  const node = Object.hasOwn(project.compositions,compositionId) ? project.compositions[compositionId] : undefined
  if (!node) throw new Error('Композиция не найдена')
  const roles = new Set(node.content.metadata?.captionTextTracks ?? [])
  const selected = new Set(unique)
  const lanes = node.content.tracks.filter(track=>track.clips.some(clip=>selected.has(clip.id)))
  if (lanes.length !== 1) throw new Error('Субтитры должны находиться на одной дорожке одной композиции')
  const lane = lanes[0]!
  if (lane.kind !== 'text' || !roles.has(lane.id)) throw new Error('Выбранные клипы не являются субтитрами')
  if (lane.locked) throw new Error('Дорожка субтитров заблокирована')
  const clips = lane.clips.filter(clip=>selected.has(clip.id)).sort((a,b)=>a.timelineStartTick-b.timelineStartTick)
  if (clips.length !== unique.length) throw new Error('Выбранный субтитр не найден в композиции')
  const first = clips[0]!, end = Math.max(...clips.map(clip=>clip.timelineEndTick))
  if (lane.clips.some(clip=>!selected.has(clip.id) && clip.timelineStartTick<end && clip.timelineEndTick>first.timelineStartTick)) throw new Error('Объединённый субтитр пересекается с другой репликой')
  const text = clips.map(clip=>clip.text).join('\n')
  if (Array.from(text).length > MAX_TEXT_LENGTH) throw new Error('Объединённый текст превышает лимит длины')
  const removed = clips.slice(1).map(clip=>clip.id)
  const unlinked = unlinkNativeClips(project,compositionId,removed)
  const deleted = removeNativeClips(unlinked,compositionId,removed)
  const changed = deleted.compositions[compositionId]!
  const document = {...deleted,compositions:{...deleted.compositions,[compositionId]:{...changed,content:{...changed.content,tracks:changed.content.tracks.map(track=>track.id===lane.id && track.kind==='text' ? {...track,clips:track.clips.map(clip=>clip.id===first.id ? {...clip,text,timelineEndTick:end} : clip)} : track)}}}}
  return {document,trackId:lane.id,record:{layer_id:first.id,t_start_us:first.timelineStartTick,t_end_us:end,content:text,merged:clips.length,removed}}
}
