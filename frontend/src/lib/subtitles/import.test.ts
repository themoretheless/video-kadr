import { describe, expect, it } from 'vitest'
import { parseSubtitleDocument } from './import.js'
import { MAX_SRT_BYTES } from './srt.js'

describe('subtitle document import', () => {
  it('converts WebVTT 90k timestamps to microseconds and decodes cue markup', () => {
    const cues = parseSubtitleDocument('\uFEFFWEBVTT\r\n\r\nNOTE ignored\r\ncomment\r\n\r\nspeaker\r\n00:01.125 --> 00:02.500 align:center\r\n<v Anna><b>Hello</b> &amp; &lt;world&gt;\r\n')
    expect(cues).toEqual([{ id: 'subtitle-1', startTicks: 1_125_000, endTicks: 2_500_000, text: 'Hello & <world>' }])
  })

  it('reads ASS and SSA event fields, commas and escaped newlines', () => {
    const cues = parseSubtitleDocument('[Script Info]\nTitle: test\n[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nComment: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,ignored\nDialogue: 0,0:00:01.23,0:00:03.45,Default,,0,0,0,,{\\b1}Hello, world\\Nnext\\hline\n')
    expect(cues).toEqual([{ id: 'subtitle-1', startTicks: 1_230_000, endTicks: 3_450_000, text: 'Hello, world\nnext\u00a0line' }])
  })

  it('retains SRT and timecoded TXT support', () => {
    expect(parseSubtitleDocument('1\n00:00:01,000 --> 00:00:02,000\nHello')[0]?.text).toBe('Hello')
    expect(parseSubtitleDocument('[00:00:01.000 --> 00:00:02.000] Hello')[0]?.startTicks).toBe(1_000_000)
  })

  it('refuses malformed, inverted, excessive and empty cues before mutation', () => {
    for (const event of [
      'Dialogue: 0:00:02.00,0:00:01.00,text',
      'Dialogue: 0:99:00.00,0:00:03.00,text',
      'Dialogue: 0:00:01.00,0:00:03.00,{\\b1}',
      `Dialogue: 0:00:01.00,0:00:03.00,${'x'.repeat(513)}`,
    ]) expect(() => parseSubtitleDocument(`[Events]\nFormat: Start, End, Text\n${event}`)).toThrow()
    expect(() => parseSubtitleDocument('[Events]\nDialogue: 0,1,text')).toThrow()
    expect(() => parseSubtitleDocument('x'.repeat(MAX_SRT_BYTES + 1))).toThrow()
  })
})
