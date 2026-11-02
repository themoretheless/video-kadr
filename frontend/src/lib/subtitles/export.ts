import type { NativeNestedProject } from '../api'
import type { Composition, TextClip } from '../composition/types'
import { formatSrt, type SubtitleCue } from './srt'

export type SubtitleExportFormat = 'srt' | 'vtt'

/** Plain editable Text layers on caption-role lanes, including hidden/locked lanes. */
export function captionExportCues(document: Composition, trackId?: string): SubtitleCue[] {
  const roles = new Set(document.metadata?.captionTextTracks ?? [])
  if (trackId && !document.tracks.some(track => track.id === trackId && track.kind === 'text' && roles.has(track.id))) {
    throw new Error('Выбранная дорожка не является дорожкой субтитров')
  }
  return document.tracks
    .filter(track => track.kind === 'text' && roles.has(track.id) && (!trackId || track.id === trackId))
    .flatMap(track => (track.clips as readonly TextClip[]).map(clip => ({
      id: clip.id, startTicks: clip.timelineStartTicks,
      endTicks: clip.timelineStartTicks + clip.durationTicks, text: clip.text,
    })))
    .sort((a, b) => a.startTicks - b.startTicks)
}

/** Composition-local authored clocks, never multiplied or shifted by group instances. */
export function nestedCaptionExportCues(project: NativeNestedProject, compositionId = project.root, trackId?: string): SubtitleCue[] {
  const composition = Object.hasOwn(project.compositions, compositionId) ? project.compositions[compositionId] : undefined
  if (!composition) throw new Error('Композиция для экспорта субтитров не найдена')
  const roles = new Set(composition.content.metadata?.captionTextTracks ?? [])
  if (trackId && !composition.content.tracks.some(track => track.id === trackId && track.kind === 'text' && roles.has(track.id))) {
    throw new Error('Выбранная дорожка не является дорожкой субтитров этой композиции')
  }
  return composition.content.tracks.flatMap(track =>
    track.kind === 'text' && roles.has(track.id) && (!trackId || track.id === trackId)
      ? track.clips.map(clip => ({id:clip.id,startTicks:clip.timelineStartTick,endTicks:clip.timelineEndTick,text:clip.text})) : [],
  ).sort((a,b) => a.startTicks - b.startTicks)
}

/** Blank lines terminate cue blocks. VTT text is markup and needs entity escaping. */
export function formatSubtitleExport(cues: readonly SubtitleCue[], format: SubtitleExportFormat): string {
  if (format !== 'srt' && format !== 'vtt') throw new Error('Поддерживается экспорт SRT или VTT')
  const normalized = cues.map(cue => ({ ...cue,
    text: cue.text.replace(/\r\n?/g, '\n').split('\n').filter(line => line.trim() !== '').join('\n'),
  }))
  // Keep shared bounds/count/text checks before writing either format.
  const srt = formatSrt(normalized)
  if (format === 'srt') return srt
  const stamp = (ticks: number) => {
    const ms = Math.round(ticks / 1000)
    return `${String(Math.floor(ms / 3600000)).padStart(2, '0')}:${String(Math.floor(ms / 60000) % 60).padStart(2, '0')}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')}.${String(ms % 1000).padStart(3, '0')}`
  }
  return 'WEBVTT\n\n' + normalized.map(cue =>
    `${stamp(cue.startTicks)} --> ${stamp(cue.endTicks)}\n${cue.text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')}\n`,
  ).join('\n')
}
