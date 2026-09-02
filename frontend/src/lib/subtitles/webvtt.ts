import type { CaptionCue, CaptionTrack } from './captions.js'
import { validateCaptionTrack } from './captions.js'

export const MAX_WEBVTT_BYTES = 2 * 1024 * 1024
export const MAX_WEBVTT_CUES = 10_000
const TICKS_PER_SECOND = 90_000

export class WebVttError extends Error {}

export function parseWebVtt(input: string, language = 'und'): CaptionTrack {
  if (new TextEncoder().encode(input).byteLength > MAX_WEBVTT_BYTES) throw new WebVttError('WebVTT exceeds byte limit')
  const normalized = input.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n')
  if (!normalized.startsWith('WEBVTT') || !/^WEBVTT(?:[ \t].*)?\n/.test(normalized)) throw new WebVttError('Missing WEBVTT signature')
  const blocks = normalized.slice(normalized.indexOf('\n') + 1).trim().split(/\n{2,}/).filter(Boolean)
  const cues: CaptionCue[] = []
  const futureHeaders: Record<string, string> = {}
  for (const block of blocks) {
    if (block.startsWith('NOTE')) continue
    if (block.startsWith('STYLE') || block.startsWith('REGION')) {
      futureHeaders[`block-${Object.keys(futureHeaders).length + 1}`] = block
      continue
    }
    const lines = block.split('\n')
    let id = `cue-${cues.length + 1}`
    if (!lines[0]!.includes('-->')) id = lines.shift()!.trim()
    const timing = lines.shift()
    if (!timing) throw new WebVttError('Cue timing is missing')
    const match = timing.match(/^(\S+)\s+-->\s+(\S+)(?:\s+(.*))?$/)
    if (!match) throw new WebVttError(`Invalid cue timing: ${id}`)
    const settings = parseSettings(match[3] ?? '')
    cues.push({ id, startTicks: parseTimestamp(match[1]!), endTicks: parseTimestamp(match[2]!), text: lines.join('\n'), language, settings })
    if (cues.length > MAX_WEBVTT_CUES) throw new WebVttError('WebVTT cue count exceeds limit')
  }
  const track: CaptionTrack = { id: 'webvtt-track', kind: 'captions', language, label: 'WebVTT', cues, regions: [] }
  validateCaptionTrack(track)
  if (Object.keys(futureHeaders).length && track.cues[0]) track.cues[0].futureMetadata = futureHeaders
  return track
}

function parseTimestamp(value: string): number {
  const match = value.match(/^(?:(\d{2,}):)?([0-5]\d):([0-5]\d)\.(\d{3})$/)
  if (!match) throw new WebVttError(`Invalid WebVTT timestamp: ${value}`)
  const hours = Number(match[1] ?? 0)
  const seconds = hours * 3600 + Number(match[2]) * 60 + Number(match[3]) + Number(match[4]) / 1000
  const ticks = Math.round(seconds * TICKS_PER_SECOND)
  if (!Number.isSafeInteger(ticks)) throw new WebVttError('WebVTT timestamp exceeds range')
  return ticks
}

function parseSettings(value: string): Record<string, string> {
  const settings: Record<string, string> = {}
  for (const token of value.split(/\s+/).filter(Boolean)) {
    const [key, setting, extra] = token.split(':')
    if (!key || !setting || extra !== undefined || !['vertical', 'line', 'position', 'size', 'align', 'region'].includes(key)) throw new WebVttError(`Invalid WebVTT cue setting: ${token}`)
    if (settings[key] !== undefined) throw new WebVttError(`Duplicate WebVTT cue setting: ${key}`)
    settings[key] = setting
  }
  return settings
}
