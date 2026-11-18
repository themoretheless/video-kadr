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

  it('applies relative ASS font sizes in order and restores the active style on reset', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[V4+ Styles]
Format: Name, Fontsize
Style: Default,40
Style: Alternate,60
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Default,{\fs50\fs+2\fs-5}Adjusted
Dialogue: 0:00:01.00,0:00:02.00,Default,{\rAlternate\fs+2\fs0}Reset
Dialogue: 0:00:02.00,0:00:03.00,Default,First{\fs+2} second
`)
    expect(report.cues.map(cue => cue.style?.fontSizePx)).toEqual([30, 60, 40])
    expect(report.simplifiedCueCount).toBe(1)
  })

  it('resets ASS RGB and opacity independently to the active style', () => {
    const [cue] = parseSubtitleDocument(String.raw`[V4+ Styles]
Format: Name, PrimaryColour, OutlineColour, BackColour, Outline, Shadow
Style: Default,&H00FFFFFF,&H00000000,&H80000000,2,1
Style: Alternate,&H4000FF00,&H200000FF,&H60FF0000,2,1
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Default,{\rAlternate\alpha&H80&\c&H0000FF&\c\3a\4c&H00FFFF&\4a}Reset
`)
    expect(cue?.style).toMatchObject({color:'#00FF007F',strokeColor:'#FF0000DF',shadowColor:'#FFFF009F'})
  })

  it('restores the active ASS face style and reports mid-line face changes', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[V4+ Styles]
Format: Name, Bold, Italic, Fontname
Style: Default,0,0,Arial
Style: Alternate,-1,0,DejaVu Sans
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Default,{\rAlternate\b0\i1\fnArial\fn\b\i}Restored
Dialogue: 0:00:01.00,0:00:02.00,Default,First{\i1\fnDejaVu Sans} second
`)
    expect(report.cues.map(cue => cue.style)).toEqual([
      {bold:true, italic:false, fontFamily:'DejaVu Sans'}, {bold:false, italic:false, fontFamily:'Arial'},
    ])
    expect(report.simplifiedCueCount).toBe(1)
  })

  it('resets ASS border and shadow geometry without changing their colors', () => {
    const [cue] = parseSubtitleDocument(String.raw`[V4+ Styles]
Format: Name, Outline, Shadow
Style: Default,2,3
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Default,{\bord8\shad5\xshad-2\3c&H0000FF&\bord\xshad}Reset
`)
    expect(cue?.style).toMatchObject({strokeWidthPx:2, shadowX:3, shadowY:5, strokeColor:'#FF0000FF'})
  })

  it('restores ASS tracking from the active style on a bare spacing tag', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[V4+ Styles]
Format: Name, Fontsize, Spacing
Style: Default,40,2
Style: Alternate,40,5
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Default,{\rAlternate\fsp-2\fsp}Restored
Dialogue: 0:00:01.00,0:00:02.00,Default,{\fsp0}Zero
`)
    expect(report.cues.map(cue => cue.style?.tracking)).toEqual([5, 0])
    expect(report.simplifiedCueCount).toBe(0)
  })

  it('restores one or both ASS scale axes from the active style', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[V4+ Styles]
Format: Name, ScaleX, ScaleY
Style: Default,100,100
Style: Alternate,120,80
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Default,{\rAlternate\fscx200\fscy150\fscx}One axis
Dialogue: 0:00:01.00,0:00:02.00,Alternate,{\fscx200\fscy150\fsc}Both axes
`)
    expect(report.cues.map(cue => cue.scale)).toEqual([{x:1.2, y:1.5}, {x:1.2, y:0.8}])
    expect(report.simplifiedCueCount).toBe(0)
  })

  it('preserves ASS karaoke modes, explicit timing and normalized text ranges', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:02.00, {\k20}One\h{\K30}два{\kt80\ko10}!
`)
    expect(report.cues[0]).toMatchObject({
      text: 'One\u00a0два!',
      karaoke: [
        {start:0,end:4,startTicks:0,endTicks:200000,mode:'step'},
        {start:4,end:7,startTicks:200000,endTicks:500000,mode:'sweep'},
        {start:7,end:8,startTicks:800000,endTicks:900000,mode:'outline'},
      ],
    })
    expect(report.simplifiedCueCount).toBe(1)
  })

  it('imports fractional and omitted ASS karaoke times on the millisecond grid', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:03.00,{\kt12.59\k.99}A{\ko}B{\kf+20.5}C{\kt\K}D
`)
    expect(report.cues[0]?.karaoke).toMatchObject([
      { startTicks: 125000, endTicks: 134000, mode: 'step' },
      { startTicks: 134000, endTicks: 1134000, mode: 'outline' },
      { startTicks: 1134000, endTicks: 1339000, mode: 'sweep' },
      { startTicks: 0, endTicks: 1000000, mode: 'sweep' },
    ])
  })

  it.each([String.raw`\kt15\k50`, String.raw`\kt( 15 )\k(50)`, String.raw`\kt 15\k 50`])(
    'imports equivalent ASS karaoke arguments %s',
    tags => {
      const report = parseSubtitleDocumentWithReport(`[Events]\nFormat: Start, End, Text\nDialogue: 0:00:00.00,0:00:02.00,{${tags}}M`)
      expect(report.cues[0]?.karaoke).toMatchObject([{ startTicks: 150000, endTicks: 650000, mode: 'step' }])
      expect(report.simplifiedCueCount).toBe(0)
    },
  )

  it('preserves escaped ASS braces and normalizes karaoke offsets around them', () => {
    const report = parseSubtitleDocumentWithReport(String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:02.00, {\k20}\{😀\}{comment}{\k30}\}end
`)
    expect(report.cues[0]).toMatchObject({
      text: '{😀}}end',
      karaoke: [
        { start: 0, end: 4, startTicks: 0, endTicks: 200000 },
        { start: 4, end: 8, startTicks: 200000, endTicks: 500000 },
      ],
    })
    expect(report.simplifiedCueCount).toBe(0)
  })

  it('preserves ASS hard spaces at cue edges and within karaoke ranges', () => {
    const [cue] = parseSubtitleDocument(String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:02.00, {\k20}\hA{\k30}B\h
`)
    expect(cue).toMatchObject({
      text: '\u00a0AB\u00a0',
      karaoke: [{ start: 0, end: 2 }, { start: 2, end: 4 }],
    })
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
