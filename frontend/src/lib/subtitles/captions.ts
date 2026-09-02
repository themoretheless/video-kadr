import { PatchCommand } from '$lib/domain/history.js'

export type TimedTextKind = 'captions' | 'chapters' | 'descriptions'

export interface CaptionRegion {
  id: string
  x: number
  y: number
  width: number
  height: number
}

export interface CaptionCue {
  id: string
  startTicks: number
  endTicks: number
  text: string
  language: string
  speaker?: string
  regionId?: string
  settings?: Record<string, string>
  futureMetadata?: Record<string, string>
}

export interface CaptionTrack {
  id: string
  kind: TimedTextKind
  language: string
  label: string
  cues: CaptionCue[]
  regions: CaptionRegion[]
}

export function validateCaptionTrack(track: CaptionTrack): void {
  if (!/^[a-z0-9][a-z0-9._-]{0,63}$/i.test(track.id)) throw new Error('Invalid caption track ID')
  const ids = new Set<string>()
  const ordered = [...track.cues].sort((left, right) => left.startTicks - right.startTicks || left.id.localeCompare(right.id))
  for (const cue of ordered) {
    if (!cue.id || ids.has(cue.id)) throw new Error('Duplicate caption cue ID')
    ids.add(cue.id)
    if (!Number.isSafeInteger(cue.startTicks) || !Number.isSafeInteger(cue.endTicks) || cue.startTicks < 0 || cue.endTicks <= cue.startTicks) throw new Error(`Invalid caption range: ${cue.id}`)
    if (!cue.text.trim()) throw new Error(`Empty caption cue: ${cue.id}`)
  }
  if (track.kind === 'chapters') {
    for (let index = 1; index < ordered.length; index += 1) {
      if (ordered[index]!.startTicks < ordered[index - 1]!.endTicks) throw new Error('Chapter cues cannot overlap')
    }
  }
}

export function safeRegion(outputWidth: number, outputHeight: number, inset = 0.05): CaptionRegion {
  if (!Number.isFinite(outputWidth) || !Number.isFinite(outputHeight) || outputWidth <= 0 || outputHeight <= 0 || inset < 0 || inset >= 0.5) throw new Error('Invalid output geometry')
  return { id: 'title-safe', x: outputWidth * inset, y: outputHeight * inset, width: outputWidth * (1 - inset * 2), height: outputHeight * (1 - inset * 2) }
}

export interface FontArtifact {
  id: string
  family: string
  sha256: string
  bytes: number
  license: string
  fallback: string[]
}

export function validateFontArtifact(font: FontArtifact): boolean {
  return /^[a-f0-9]{64}$/i.test(font.sha256) && font.bytes > 0 && font.bytes <= 16 * 1024 * 1024 && Boolean(font.family.trim()) && Boolean(font.license.trim()) && font.fallback.length <= 8
}

export interface ReferenceSubtitleRender {
  renderer: 'libass'
  rendererVersion: string
  sourceSha256: string
  fontSha256: string[]
  imageSha256: string
  warnings: string[]
}

export function validateReferenceRender(render: ReferenceSubtitleRender): boolean {
  return Boolean(render.rendererVersion) && [render.sourceSha256, render.imageSha256, ...render.fontSha256].every((hash) => /^[a-f0-9]{64}$/i.test(hash))
}

export type ReadabilityFindingKind = 'characters-per-second' | 'line-length' | 'line-count' | 'minimum-gap'
export interface ReadabilityFinding { cueId: string; kind: ReadabilityFindingKind; actual: number; limit: number }
export interface ReadabilityPolicy { ticksPerSecond: number; maxCharactersPerSecond: number; maxLineLength: number; maxLines: number; minimumGapTicks: number }

export function lintReadability(track: CaptionTrack, policy: ReadabilityPolicy): ReadabilityFinding[] {
  const findings: ReadabilityFinding[] = []
  const cues = [...track.cues].sort((left, right) => left.startTicks - right.startTicks)
  cues.forEach((cue, index) => {
    const characters = Array.from(cue.text.replace(/\s/g, '')).length
    const seconds = (cue.endTicks - cue.startTicks) / policy.ticksPerSecond
    const cps = characters / seconds
    if (cps > policy.maxCharactersPerSecond) findings.push({ cueId: cue.id, kind: 'characters-per-second', actual: cps, limit: policy.maxCharactersPerSecond })
    const lines = cue.text.split('\n')
    const lineLength = Math.max(...lines.map((line) => Array.from(line).length))
    if (lineLength > policy.maxLineLength) findings.push({ cueId: cue.id, kind: 'line-length', actual: lineLength, limit: policy.maxLineLength })
    if (lines.length > policy.maxLines) findings.push({ cueId: cue.id, kind: 'line-count', actual: lines.length, limit: policy.maxLines })
    const previous = cues[index - 1]
    if (previous && cue.startTicks - previous.endTicks < policy.minimumGapTicks) findings.push({ cueId: cue.id, kind: 'minimum-gap', actual: cue.startTicks - previous.endTicks, limit: policy.minimumGapTicks })
  })
  return findings
}

export function cueTransaction(before: CaptionTrack, after: CaptionTrack, gesture: string): PatchCommand<CaptionTrack> {
  validateCaptionTrack(after)
  const command = PatchCommand.between(before, after, `caption:${gesture}`)
  if (!command) throw new Error('Caption gesture produced no change')
  return command
}

export interface CueAlignment { sourceCueId: string; translatedCueId: string | null; confidence?: number }
export interface LinkedTranslation { sourceTrackId: string; translatedTrackId: string; alignments: CueAlignment[] }

export function validateTranslation(link: LinkedTranslation, source: CaptionTrack, translated: CaptionTrack): string[] {
  if (link.sourceTrackId !== source.id || link.translatedTrackId !== translated.id) throw new Error('Translation track identity mismatch')
  const sourceIds = new Set(source.cues.map((cue) => cue.id))
  const translatedIds = new Set(translated.cues.map((cue) => cue.id))
  const unmatched = new Set(sourceIds)
  for (const alignment of link.alignments) {
    if (!sourceIds.has(alignment.sourceCueId)) throw new Error('Unknown source cue alignment')
    unmatched.delete(alignment.sourceCueId)
    if (alignment.translatedCueId && !translatedIds.has(alignment.translatedCueId)) throw new Error('Unknown translated cue alignment')
  }
  return [...unmatched]
}

export interface AccessibleMediaAudit {
  findings: { code: string; severity: 'info' | 'warning' | 'error'; targetId?: string }[]
  manualReview: string[]
}

export function auditAccessibleMedia(tracks: readonly CaptionTrack[], controlsKeyboardOperable: boolean): AccessibleMediaAudit {
  const findings: AccessibleMediaAudit['findings'] = []
  if (!tracks.some((track) => track.kind === 'captions')) findings.push({ code: 'captions-missing', severity: 'warning' })
  for (const track of tracks) {
    if (!track.language.trim()) findings.push({ code: 'language-missing', severity: 'error', targetId: track.id })
  }
  if (!controlsKeyboardOperable) findings.push({ code: 'controls-keyboard-inoperable', severity: 'error' })
  return { findings, manualReview: ['caption accuracy', 'audio description quality', 'speaker identification'] }
}
