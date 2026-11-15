import type { SubtitleDocument } from '../subtitles/documents'

export function subtitleApplication(documents: readonly SubtitleDocument[] | undefined, args: Record<string, unknown>) {
  if (Object.keys(args).some(key => !['expectedRevision', 'expectedProjectHash', 'composition_id', 'media_id', 't_start_us'].includes(key))) {
    throw new Error('Invalid apply_subtitles argument')
  }
  const id = args.media_id
  if (typeof id !== 'string' || !id || id.length > 128) throw new Error('Invalid media_id')
  const offset = args.t_start_us === undefined ? 0 : args.t_start_us
  if (typeof offset !== 'number' || !Number.isSafeInteger(offset) || offset < 0) throw new Error('Invalid t_start_us')
  const source = documents?.find(document => document.id === id)
  if (!source) throw new Error('Subtitle document not found in this project')
  return { source, offset }
}
