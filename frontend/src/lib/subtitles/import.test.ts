import { describe, expect, it } from 'vitest'
import { parseSubtitleDocument, parseSubtitleDocumentWithReport } from './import.js'
import { cuesToTextClips, MAX_SRT_BYTES } from './srt.js'

describe('subtitle document import', () => {
  it('converts WebVTT 90k timestamps to microseconds and decodes cue markup', () => {
    const cues = parseSubtitleDocument('\uFEFFWEBVTT\r\n\r\nNOTE ignored\r\ncomment\r\n\r\nspeaker\r\n00:01.125 --> 00:02.500 align:center\r\n<v Anna><b>Hello</b> &amp; &lt;world&gt;\r\n')
    expect(cues).toEqual([{ id: 'subtitle-1', startTicks: 1_125_000, endTicks: 2_500_000, text: 'Hello & <world>' }])
  })

  it('reads ASS and SSA event fields, commas and escaped newlines', () => {
    const cues = parseSubtitleDocument('[Script Info]\nTitle: test\n[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nComment: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,ignored\nDialogue: 0,0:00:01.23,0:00:03.45,Default,,0,0,0,,{\\b1}Hello, world\\Nnext\\hline\n')
    expect(cues).toEqual([{ id: 'subtitle-1', startTicks: 1_230_000, endTicks: 3_450_000, text: 'Hello, world\nnext\u00a0line', style: { bold: true } }])
  })

  it('retains SRT and timecoded TXT support', () => {
    expect(parseSubtitleDocument('1\n00:00:01,000 --> 00:00:02,000\nHello')[0]?.text).toBe('Hello')
    expect(parseSubtitleDocument('[00:00:01.000 --> 00:00:02.000] Hello')[0]?.startTicks).toBe(1_000_000)
  })

  it('applies named ASS styles and cue overrides to editable text clips', () => {
    const cues = parseSubtitleDocument(String.raw`[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, OutlineColour, BackColour, Outline, Shadow, Alignment
Style: Default,Arial,64,&H40402010,&H20000000,&H80000000,2,1,2

[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:02.00,Default,{\an7\c&H0000FF&\fs56\fnDejaVu Sans}Styled cue
Dialogue: 0:00:02.00,0:00:04.00,Default,Named style
`)
    const [overridden, named] = cuesToTextClips(cues, cue => cue.id)
    expect(overridden?.text).toBe('Styled cue')
    expect(overridden?.style).toMatchObject({
      fontFamily: 'DejaVu Sans', fontSizePx: 56, color: '#FF0000BF',
      align: 'left', strokeColor: '#000000DF', strokeWidthPx: 2, shadowX: 1, shadowY: 1,
      shadowColor: '#0000007F',
    })
    expect(named?.style).toMatchObject({ fontFamily: 'Arial', fontSizePx: 64, color: '#102040BF', align: 'center' })
  })

  it('resets leading ASS overrides to the selected style and keeps line alignment', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[V4+ Styles]
Format: Name, Fontname, Fontsize, Alignment
Style: Default,Arial,40,2
Style: Alternate,DejaVu Sans,48,7
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Default,{\i1\rAlternate\b1}Alternate
Dialogue: 0:00:01.00,0:00:02.00,Default,{\rAlternate\r}Default
Dialogue: 0:00:02.00,0:00:03.00,Default,First{\rAlternate} second
`)
    expect(report.cues[0]).toMatchObject({ style: { fontFamily: 'DejaVu Sans', fontSizePx: 48, align: 'center', bold: true }, anchor: [0.5, 1] })
    expect(report.cues[0]?.style).not.toHaveProperty('italic')
    expect(report.cues[1]?.style).toMatchObject({ fontFamily: 'Arial', fontSizePx: 40 })
    expect(report.cues[2]).toMatchObject({ text: 'First second', style: { fontFamily: 'Arial' } })
    expect(report.simplifiedCueCount).toBe(1)
  })

  it('imports legacy SSA styles with decimal colors and the legacy alignment grid', () => {
    const [cue] = parseSubtitleDocument(`[V4 Styles]
Style: Default,Arial,48,255,0,0,16711680,-1,0,1,2,1,5,0,0,0,64,1
[Events]
Format: Marked, Start, End, Style, Text
Dialogue: Marked=0,0:00:00.00,0:00:01.00,Default,Legacy
`)
    expect(cue).toMatchObject({text: 'Legacy', anchor: [0, 0], style: {
      fontSizePx: 48, align: 'left', color: '#FF0000BF',
      strokeColor: '#0000FFBF', shadowColor: '#0000FF7F',
    }})
  })

  it.each(['[Script Info]\nScriptType: v4.00', '[V4 Styles]'])('uses the default SSA event fields after %j', header => {
    const [cue] = parseSubtitleDocument(`${header}
[Events]
Dialogue: Marked=0,0:00:01.25,0:00:02.50,Default,Actor,0000,0000,0000,,Hello, world
`)
    expect(cue).toMatchObject({startTicks: 1_250_000, endTicks: 2_500_000, text: 'Hello, world'})
  })

  it('uses the authored Default style for blank and unresolved ASS style references', () => {
    const cues = parseSubtitleDocument(String.raw`[V4+ Styles]
Format: Name, Fontname, Fontsize, Alignment
Style: Default,Arial,48,8
Style: Alternate,DejaVu Sans,24,2
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,,Empty
Dialogue: 0:00:01.00,0:00:02.00,Missing,{\fs200\r}Unknown
Dialogue: 0:00:02.00,0:00:03.00,Alternate,Explicit
`)
    for (const cue of cues.slice(0, 2)) {
      expect(cue).toMatchObject({anchor: [0.5, 0], style: {fontFamily: 'Arial', fontSizePx: 48}})
    }
    expect(cues[2]).toMatchObject({anchor: [0.5, 1], style: {fontFamily: 'DejaVu Sans', fontSizePx: 24}})
  })

  it.each([
    ['Title: No explicit resolution', 384, 288],
    ['PlayResX: 1280', 1280, 1024], ['PlayResY: 1024', 1280, 1024],
    ['PlayResX: 642', 642, 481], ['PlayResY: 480', 640, 480],
  ])('infers the missing ASS script dimension from %s', (field, width, height) => {
    const [cue] = parseSubtitleDocument(`[Script Info]
${field}
[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:01.00,{\\pos(100,100)}Caption
`)
    expect(cue?.coordinateResolution).toEqual({width, height})
  })

  it('uses the first ASS line alignment when legacy and modern tags are mixed', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:01.00,{\a10}Middle{\an3}
Dialogue: 0:00:01.00,0:00:02.00,{\an9\a5}Top right
Dialogue: 0:00:02.00,0:00:03.00,{\a8}Legacy edge
`)
    expect(report.simplifiedCueCount).toBe(0)
    expect(report.cues.map(cue => [cue.anchor, cue.style?.align])).toEqual([
      [[0.5,0.5], 'center'], [[1,0], 'right'], [[0,0], 'left'],
    ])
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

  it('applies leading ASS alpha overrides by channel and reports span-local alpha', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:01.00,{\alpha&H80&}{\1a&HFF&\c&H0000FF&\3a&H20&}Styled
Dialogue: 0:00:01.00,0:00:02.00,{\4a&H40&}First{\alpha&HFF&} second
`)
    expect(report.simplifiedCueCount).toBe(1)
    expect(report.cues[0]?.style).toEqual({
      color: '#FF000000', strokeColor: '#000000DF', shadowColor: '#0000007F',
    })
    expect(report.cues[1]).toMatchObject({text: 'First second', style: {shadowColor: '#000000BF'}})
    expect(report.cues[1]?.style).not.toHaveProperty('color')
  })

  it('reports skipped effects per cue without applying their nested commands', () => {
    const result = parseSubtitleDocumentWithReport(String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:01.00,{\k50\t(0,300,\fs80\b1)\i1}Still readable
Dialogue: 0:00:01.00,0:00:02.00,{\b1}Supported
`)
    expect(result.simplifiedCueCount).toBe(1)
    expect(result.cues[0]).toMatchObject({ text: 'Still readable', startTicks: 0, endTicks: 1_000_000, style: { italic: true } })
    expect(result.cues[0]?.style).not.toHaveProperty('fontSizePx')
    expect(result.cues[0]?.style).not.toHaveProperty('bold')
    expect(result.cues[1]?.style).toEqual({ bold: true })
  })
})
