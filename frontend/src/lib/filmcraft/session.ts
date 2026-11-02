export interface FilmcraftDurationSetting { value: string; unit: 'frames' | 'seconds' }
export interface FilmcraftTransitionSettings { video: { effect: string; duration: FilmcraftDurationSetting }; audio: { effect: string; duration: FilmcraftDurationSetting }; single: FilmcraftDurationSetting }
export function validTransitionDurationText(value: string): boolean { return value.length > 0 && value.length <= 64 && value.trim() === value && /^[+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value) && Number.isFinite(Number(value)) && Number(value) >= 0 }
function durationSetting(value: unknown): value is FilmcraftDurationSetting { if (!value || typeof value !== 'object') return false; const v = value as Record<string, unknown>; return typeof v.value === 'string' && validTransitionDurationText(v.value) && (v.unit === 'frames' || v.unit === 'seconds') }
function transitionSettings(value: unknown): value is FilmcraftTransitionSettings {
  if (!value || typeof value !== 'object') return false
  const settings = value as Record<string, unknown>
  return ['video', 'audio'].every((key) => { const item = settings[key] as Record<string, unknown> | undefined; return !!item && typeof item.effect === 'string' && /^[a-z0-9_]{1,128}$/.test(item.effect) && durationSetting(item.duration) }) && durationSetting(settings.single)
}
/** Portable host session; source document remains an opaque JSON string. */
export interface FilmcraftSession {
  format: 'video-kadr.filmcraft-session'
  version: 1
  documentText: string
  sequenceId: string
  sources: Record<string, string>
  start: number
  end: number
  audioFormat: 'wav' | 'aiff'
  bits: 16 | 24
  channels: 1 | 2
  transitionSettings?: FilmcraftTransitionSettings
}
const id = /^(0|[1-9][0-9]{0,19})$/
function sourceId(value: unknown): value is string {
  return typeof value === 'string' && id.test(value) && BigInt(value) <= 18446744073709551615n
}
export function parseFilmcraftSession(text: string): FilmcraftSession | null {
  const input: unknown = JSON.parse(text)
  if (!input || typeof input !== 'object' || !('format' in input) || input.format !== 'video-kadr.filmcraft-session') return null
  const data = input as Record<string, unknown>
  if (data.version !== 1 || typeof data.documentText !== 'string' || data.documentText.length > 24 * 1024 * 1024 || !sourceId(data.sequenceId)
    || typeof data.start !== 'number' || !Number.isFinite(data.start) || data.start < 0
    || typeof data.end !== 'number' || !Number.isFinite(data.end) || data.end <= data.start
    || !['wav', 'aiff'].includes(data.audioFormat as string) || ![16, 24].includes(data.bits as number) || ![1, 2].includes(data.channels as number)
    || !data.sources || typeof data.sources !== 'object' || Array.isArray(data.sources)) throw new Error('Некорректная сессия Filmcraft')
  const entries = Object.entries(data.sources)
  if (entries.length > 64 || entries.some(([key, value]) => !sourceId(key) || typeof value !== 'string' || !value || value.length > 256)) throw new Error('Некорректные привязки медиафайлов')
  if (data.transitionSettings !== undefined && !transitionSettings(data.transitionSettings)) throw new Error('Некорректные настройки переходов')
  return { format: 'video-kadr.filmcraft-session', version: 1, documentText: data.documentText, sequenceId: data.sequenceId, sources: Object.fromEntries(entries) as Record<string, string>, start: data.start, end: data.end, audioFormat: data.audioFormat as 'wav' | 'aiff', bits: data.bits as 16 | 24, channels: data.channels as 1 | 2, ...(data.transitionSettings === undefined ? {} : { transitionSettings: data.transitionSettings as FilmcraftTransitionSettings }) }
}
export function encodeFilmcraftSession(session: FilmcraftSession): string {
  const text = JSON.stringify(session)
  if (!parseFilmcraftSession(text)) throw new Error('Некорректная сессия Filmcraft')
  return text
}
