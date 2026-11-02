import { expect, it } from 'vitest'
import { formatSubtitleExport } from './export'
import { parseSubtitleDocument } from './import'
const cues = [
  { id: 'a', startTicks: 1_000_000, endTicks: 3_000_000, text: 'Hello\r\n\r\nПривет' },
  { id: 'b', startTicks: 2_000_000, endTicks: 4_000_000, text: 'A & <B> --> C' },
]
it('writes source-equivalent SRT and markup-safe VTT with overlapping times', () => {
  const srt = formatSubtitleExport(cues, 'srt')
  expect(srt).toBe('1\n00:00:01,000 --> 00:00:03,000\nHello\nПривет\n\n2\n00:00:02,000 --> 00:00:04,000\nA & <B> --> C\n')
  const vtt = formatSubtitleExport(cues, 'vtt')
  expect(vtt).toBe('WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHello\nПривет\n\n00:00:02.000 --> 00:00:04.000\nA &amp; &lt;B&gt; --&gt; C\n')
  expect(parseSubtitleDocument(vtt).map(({startTicks,endTicks,text})=>({startTicks,endTicks,text})))
    .toEqual(cues.map(({startTicks,endTicks,text})=>({startTicks,endTicks,text:text.replace(/\r\n\r\n/g,'\n')})))
})
it('keeps source millisecond rounding across minute rollover and empty output', () => {
  expect(formatSubtitleExport([{id:'a',startTicks:59_999_500,endTicks:60_001_000,text:'X'}],'vtt'))
    .toContain('00:01:00.000 --> 00:01:00.001')
  expect(formatSubtitleExport([], 'srt')).toBe('')
  expect(formatSubtitleExport([], 'vtt')).toBe('WEBVTT\n\n')
})
it('rejects invalid text, bounds and unsupported formats', () => {
  expect(()=>formatSubtitleExport([{...cues[0]!,text:'\r\n  \n'}],'srt')).toThrow()
  expect(()=>formatSubtitleExport([{...cues[0]!,endTicks:0}],'vtt')).toThrow()
  expect(()=>formatSubtitleExport(cues,'ass' as 'srt')).toThrow()
})
