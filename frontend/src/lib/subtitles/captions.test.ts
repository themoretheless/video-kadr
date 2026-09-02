import { describe, expect, it } from 'vitest'
import { auditAccessibleMedia, cueTransaction, lintReadability, safeRegion, validateCaptionTrack, validateFontArtifact, validateTranslation, type CaptionTrack } from './captions.js'
import { parseWebVtt, WebVttError } from './webvtt.js'

const track = (): CaptionTrack => ({ id: 'captions-en', kind: 'captions', language: 'en', label: 'English', regions: [], cues: [{ id: 'cue-a', startTicks: 0, endTicks: 90_000, text: 'Hello', language: 'en' }, { id: 'cue-b', startTicks: 91_000, endTicks: 180_000, text: 'A line that is intentionally long', language: 'en' }] })

describe('caption contracts', () => {
  it('parses bounded strict WebVTT and preserves future non-executable metadata', () => {
    const parsed = parseWebVtt('WEBVTT\n\nREGION\nid:bottom\n\nintro\n00:00:00.000 --> 00:00:01.000 align:center\nHello', 'en')
    expect(parsed.cues[0]).toMatchObject({ id: 'intro', startTicks: 0, endTicks: 90_000, settings: { align: 'center' } })
    expect(parsed.cues[0]?.futureMetadata).toBeDefined()
    expect(() => parseWebVtt('WEBVTT\n\n00:00:00.000 --> 00:00:01.000 evil:true\nX')).toThrow(WebVttError)
  })

  it('allows caption overlap but rejects chapter overlap and computes aspect-safe guides', () => {
    const captions = track()
    captions.cues[1]!.startTicks = 80_000
    expect(() => validateCaptionTrack(captions)).not.toThrow()
    expect(() => validateCaptionTrack({ ...captions, kind: 'chapters' })).toThrow('overlap')
    expect(safeRegion(1080, 1920)).toEqual({ id: 'title-safe', x: 54, y: 96, width: 972, height: 1728 })
  })

  it('produces readability findings and one invertible gesture command', () => {
    const before = track()
    const after = structuredClone(before)
    after.cues[0]!.endTicks = 120_000
    const command = cueTransaction(before, after, 'drag-end')
    expect(command.apply(before)).toEqual(after)
    expect(command.invert().apply(after)).toEqual(before)
    expect(lintReadability(before, { ticksPerSecond: 90_000, maxCharactersPerSecond: 20, maxLineLength: 20, maxLines: 2, minimumGapTicks: 2_000 }).map((finding) => finding.kind)).toContain('line-length')
  })

  it('links translations by stable cue ID and separates automated/manual audit', () => {
    const source = track()
    const translated = { ...track(), id: 'captions-ru', language: 'ru', cues: track().cues.map((cue) => ({ ...cue, id: `ru-${cue.id}`, language: 'ru' })) }
    expect(validateTranslation({ sourceTrackId: source.id, translatedTrackId: translated.id, alignments: [{ sourceCueId: 'cue-a', translatedCueId: 'ru-cue-a' }] }, source, translated)).toEqual(['cue-b'])
    const audit = auditAccessibleMedia([source], false)
    expect(audit.findings).toContainEqual({ code: 'controls-keyboard-inoperable', severity: 'error' })
    expect(audit.manualReview).toContain('caption accuracy')
    expect(validateFontArtifact({ id: 'font-a', family: 'Inter', sha256: 'a'.repeat(64), bytes: 1000, license: 'OFL-1.1', fallback: ['sans-serif'] })).toBe(true)
  })
})
