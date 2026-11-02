import { exactCaptionDeltaTicks } from './captionTime.js'
import { transitionSourceSpan } from './transitionTimeline.js'
import { findClipLocation, freezeSourceRange, shiftClipAutomation } from './commands.js'
import { linkedClipIds } from './links.js'
import { assertCompositionLocksPreserved } from './locks.js'
import { clipEndTicks, type Composition, type CompositionClip, type CompositionTrack } from './types.js'
import { assertValidComposition } from './validation.js'

function stable(value: unknown): string {
  const normalize = (v: unknown): unknown => Array.isArray(v) ? v.map(normalize) : v && typeof v === 'object'
    ? Object.fromEntries(Object.entries(v).filter(([, x]) => x !== undefined).sort(([a], [b]) => a.localeCompare(b)).map(([k, x]) => [k, normalize(x)])) : v
  return JSON.stringify(normalize(value))
}
function settings(clip: CompositionClip, offset: number): string {
  const rebased = shiftClipAutomation(clip, offset)
  return stable(Object.fromEntries(Object.entries(rebased).filter(([key]) => !['id', 'timelineStartTicks', 'timelineDurationTicks', 'durationTicks', 'sourceInTicks', 'sourceOutTicks', 'fadeInTicks', 'fadeOutTicks'].includes(key))))
}

/** Lossless joining of source-continuous fragments; consolidation is a separate render operation. */
export function mergeClips(document: Composition, selected: readonly string[]): { document: Composition; survivors: string[] } {
  assertValidComposition(document)
  if (selected.length < 2 || new Set(selected).size !== selected.length) throw new Error('Выберите не менее двух разных фрагментов')
  const ids = new Set(selected.flatMap(id => linkedClipIds(document, id)))
  for (const id of ids) {
    const { track, clip } = findClipLocation(document, id)
    if (track.locked || clip.locked) throw new Error('Фрагмент или связанная дорожка заблокированы')
  }
  const remap = new Map<string, string>()
  const survivors: string[] = []
  const tracks = document.tracks.map(track => {
    const pieces = track.clips.filter(clip => ids.has(clip.id)).sort((a, b) => a.timelineStartTicks - b.timelineStartTicks)
    if (pieces.length < 2) return track
    const first = pieces[0]!, last = pieces.at(-1)!
    if (track.kind === 'video') for (const transition of track.transitions ?? []) {
      const outgoing = transition.fromClipId !== null && ids.has(transition.fromClipId)
      const incoming = transition.toClipId !== null && ids.has(transition.toClipId)
      if (!outgoing && !incoming) continue
      if (outgoing && incoming) throw new Error('Переход внутри объединения требует отдельного рендера')
      if (outgoing && transition.fromClipId !== last.id || incoming && transition.toClipId !== first.id) throw new Error('Переход привязан к внутреннему фрагменту')
      const cut = outgoing ? clipEndTicks(last) : first.timelineStartTicks
      const [start, duration] = transition.timeline ? transitionSourceSpan(transition.timeline)
        : [exactCaptionDeltaTicks(cut - Math.floor(transition.durationTicks / 2)), exactCaptionDeltaTicks(transition.durationTicks)]
      if (outgoing && start < exactCaptionDeltaTicks(last.timelineStartTicks) || incoming && start + duration > exactCaptionDeltaTicks(clipEndTicks(first))) throw new Error('Окно перехода пересекает внутреннюю границу фрагментов')
    }
    if (track.clips.some(c => !ids.has(c.id) && c.timelineStartTicks < clipEndTicks(last) && clipEndTicks(c) > first.timelineStartTicks)) throw new Error('Внутри выбранного диапазона есть другие клипы')
    const baseline = settings(first, 0)
    for (let i = 0; i < pieces.length; i++) {
      const clip = pieces[i]!, previous = pieces[i - 1]
      if (clip.kind !== first.kind || settings(clip, clip.timelineStartTicks - first.timelineStartTicks) !== baseline) throw new Error('Настройки и анимация фрагментов различаются')
      if ('effects' in clip && clip.effects?.length) throw new Error('Объединение обработанных слоёв требует сохранения часов эффектов')
      if (clip.kind === 'graphic' && (clip.responsiveTime || clip.roll && clip.roll.mode !== 'off')) throw new Error('Графика с изменяемым временем требует отдельной сборки')
      if ((clip.kind === 'video' || clip.kind === 'audio') && clip.speedRamp) throw new Error('Объединение кривых скорости требует отдельной обработки')
      if (clip.kind === 'audio' && ((i > 0 && clip.fadeInTicks) || (i < pieces.length - 1 && clip.fadeOutTicks) || clip.crossfadeInTicks)) throw new Error('Внутри объединения есть аудиопереход')
      if (previous && clipEndTicks(previous) !== clip.timelineStartTicks) throw new Error('Между фрагментами есть пробел или наложение')
      if (previous && !(clip.kind === 'video' && clip.playbackMode?.mode === 'freeze') && (clip.kind === 'video' || clip.kind === 'audio') && (previous.kind === 'video' || previous.kind === 'audio')) {
        const reverse = clip.kind === 'video' ? clip.playbackMode?.mode === 'reverse' : clip.reversed
        if (reverse ? previous.sourceInTicks !== clip.sourceOutTicks : previous.sourceOutTicks !== clip.sourceInTicks) throw new Error('Порядок кадров исходника нарушен')
      }
      if (stable(document.metadata?.mediaVersions?.[clip.id]) !== stable(document.metadata?.mediaVersions?.[first.id])) throw new Error('Сохранённые версии медиа различаются')
      const firstName = document.metadata?.clipNames?.[first.id], name = document.metadata?.clipNames?.[clip.id]
      if (name !== firstName) throw new Error('Названия фрагментов различаются')
      remap.set(clip.id, first.id)
    }
    const duration = clipEndTicks(last) - first.timelineStartTicks
    let merged: CompositionClip
    if ((first.kind === 'video' || first.kind === 'audio') && (last.kind === 'video' || last.kind === 'audio')) {
      merged = { ...first, sourceInTicks: Math.min(...pieces.map(c => 'sourceInTicks' in c ? c.sourceInTicks : 0)), sourceOutTicks: Math.max(...pieces.map(c => 'sourceOutTicks' in c ? c.sourceOutTicks : 0)), timelineDurationTicks: duration }
      if (first.kind === 'video' && first.playbackMode?.mode === 'freeze') merged = { ...first, ...freezeSourceRange(document, first, duration), timelineDurationTicks: duration }
      if (merged.kind === 'audio' && last.kind === 'audio') merged = { ...merged, fadeOutTicks: last.fadeOutTicks }
    } else if ('durationTicks' in first) merged = { ...first, durationTicks: duration }
    else throw new Error('Неизвестный тип фрагмента')
    survivors.push(first.id)
    return { ...track, clips: track.clips.filter(c => !remap.has(c.id) || c.id === first.id).map(c => c.id === first.id ? merged : c),
      ...(track.kind === 'video' && track.transitions ? { transitions: track.transitions.map(t => ({ ...t,
        fromClipId: t.fromClipId === null ? null : remap.get(t.fromClipId) ?? t.fromClipId,
        toClipId: t.toClipId === null ? null : remap.get(t.toClipId) ?? t.toClipId,
      })) } : {}),
    } as CompositionTrack
  })
  if (!survivors.length) throw new Error('Для объединения нужны соседние фрагменты одной дорожки')
  // Rejoin corresponding AV fragments and retain every other link member.
  const links: { id: string; memberIds: string[] }[] = []
  for (const link of document.links ?? []) {
    let members = [...new Set(link.memberIds.map(id => remap.get(id) ?? id))]
    let id = link.id
    for (let i = links.length - 1; i >= 0; i--) if (links[i]!.memberIds.some(member => members.includes(member))) {
      id = links[i]!.id; members = [...new Set([...links[i]!.memberIds, ...members])]; links.splice(i, 1)
    }
    if (members.length >= 2) links.push({ id, memberIds: members })
  }
  const clipNames = document.metadata?.clipNames && Object.fromEntries(Object.entries(document.metadata.clipNames).filter(([id]) => !remap.has(id) || remap.get(id) === id))
  const mediaVersions = document.metadata?.mediaVersions && Object.fromEntries(Object.entries(document.metadata.mediaVersions).filter(([id]) => !remap.has(id) || remap.get(id) === id))
  const result = { ...document, ...(mediaVersions ? { metadata: { ...document.metadata, mediaVersions } } : {}), tracks, ...(document.links ? { links } : {}), ...(clipNames ? { metadata: { ...document.metadata, ...(mediaVersions ? { mediaVersions } : {}), clipNames } } : {}) }
  assertValidComposition(result)
  assertCompositionLocksPreserved(document, result)
  return { document: result, survivors }
}
export function mergeClipsReason(document: Composition, ids: readonly string[]): string | null {
  try { mergeClips(document, ids); return null } catch (error) { return error instanceof Error ? error.message : 'Не удалось объединить фрагменты' }
}
