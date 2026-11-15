import { changeSubtitleDocuments, validSubtitleDocuments, type SubtitleDocument } from '../subtitles/documents'
import { parseSubtitleDocument } from '../subtitles/import'

export function planSubtitleDocumentImport(
  available: readonly SubtitleDocument[] | undefined,
  args: Record<string, unknown>,
) {
  if (Object.keys(args).some(key => !['expectedRevision', 'expectedProjectHash', 'name', 'body'].includes(key))) {
    throw new Error('Invalid import_subtitle_document argument')
  }
  if (typeof args.name !== 'string' || typeof args.body !== 'string') {
    throw new Error('Subtitle document requires name and body')
  }
  const current = available ?? []
  const candidate = { id: crypto.randomUUID(), name: args.name, text: args.body }
  if (!validSubtitleDocuments([candidate])) throw new Error('Invalid subtitle document name or size')
  const duplicate = current.find(item => item.name === candidate.name && item.text === candidate.text)
  if (duplicate) return { source: duplicate, documents: current, added: false }
  let count: number
  try {
    count = parseSubtitleDocument(candidate.text).length
  } catch {
    throw new Error('Invalid subtitle document contents')
  }
  if (!count) throw new Error('Subtitle document has no cues')
  return { source: candidate, documents: changeSubtitleDocuments(current, candidate), added: true }
}
