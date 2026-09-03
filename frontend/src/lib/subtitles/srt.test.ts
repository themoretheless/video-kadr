import { describe, expect, it } from 'vitest'

import {
  cuesToTextClips,
  formatSrt,
  parseSrt,
  parseTimecodedText,
  SubtitleFormatError,
  textClipsToCues,
} from './srt'

describe('local SRT import/export', () => {
  it('parses BOM, CRLF, multiline text, dot milliseconds, and trailing cue settings', () => {
    const cues = parseSrt(
      '\uFEFF1\r\n00:00:01,250 --> 00:00:03,500\r\nПривет\r\nмир\r\n\r\n' +
        '2\r\n00:01:00.000 --> 00:01:02.125 align:center\r\nSecond',
    )
    expect(cues).toEqual([
      {
        id: 'subtitle-1',
        startTicks: 1_250_000,
        endTicks: 3_500_000,
        text: 'Привет\nмир',
      },
      {
        id: 'subtitle-2',
        startTicks: 60_000_000,
        endTicks: 62_125_000,
        text: 'Second',
      },
    ])
  })

  it('round-trips timestamps and Unicode within one millisecond', () => {
    const source = [
      { id: 'a', startTicks: 1_234_499, endTicks: 5_678_501, text: 'Привет 👋' },
      { id: 'b', startTicks: 60_000_000, endTicks: 61_000_000, text: 'line 1\nline 2' },
    ]
    const decoded = parseSrt(formatSrt(source))
    expect(Math.abs(decoded[0]!.startTicks - source[0]!.startTicks)).toBeLessThanOrEqual(500)
    expect(Math.abs(decoded[0]!.endTicks - source[0]!.endTicks)).toBeLessThanOrEqual(500)
    expect(decoded.map((cue) => cue.text)).toEqual(source.map((cue) => cue.text))
  })

  it('maps cues to editable text clips and back in timeline order', () => {
    const cues = parseSrt('1\n00:00:02,000 --> 00:00:03,000\nTwo\n\n2\n00:00:00,000 --> 00:00:01,000\nZero')
    const clips = cuesToTextClips(cues, (_cue, index) => `clip-${index}`)
    expect(clips[0]!.style.align).toBe('center')
    expect(textClipsToCues(clips).map((cue) => cue.text)).toEqual(['Zero', 'Two'])
  })

  it('rejects malformed ranges, empty captions, and pathologically long text', () => {
    expect(() => parseSrt('1\n00:00:02,000 --> 00:00:01,000\nBackwards')).toThrow(
      SubtitleFormatError,
    )
    expect(() => parseSrt('1\n00:00:00,000 --> 00:00:01,000\n')).toThrow(
      SubtitleFormatError,
    )
    expect(() =>
      parseSrt(`1\n00:00:00,000 --> 00:00:01,000\n${'x'.repeat(513)}`),
    ).toThrow(SubtitleFormatError)
  })

  it('parses timecoded TXT lines with brackets, dots and Unicode', () => {
    expect(parseTimecodedText(
      '[00:00:00.000 --> 00:00:01.250] Привет 👋\n00:00:02,000 --> 00:00:03,000 Мир',
    )).toMatchObject([
      { startTicks: 0, endTicks: 1_250_000, text: 'Привет 👋' },
      { startTicks: 2_000_000, endTicks: 3_000_000, text: 'Мир' },
    ])
  })
})
