import { CONCAT_CAPTION_CLIPS, MAX_CONCAT_CAPTION_BYTES } from './concatTemplateCaptions'
import { CONCAT_SUBTITLE_SIDECAR, MAX_SUBTITLE_SIDECAR_BYTES } from '../subtitles/documents'
import type { FileEntry } from '@zip.js/zip.js'

const MiB = 1024 * 1024
const MAX_CONTENT = 2 * 1024 * MiB + 20 * MiB + MAX_SUBTITLE_SIDECAR_BYTES + MAX_CONCAT_CAPTION_BYTES

/** Extract a single Concat bundle without writing paths to the local filesystem. */
export async function readConcatTemplateZip(archive: File, signal: AbortSignal): Promise<File[]> {
  if (archive.size > MAX_CONTENT + MiB) throw new Error('ZIP-шаблон Concat слишком большой')
  const { BlobReader, ZipReader } = await import('@zip.js/zip.js')
  signal.throwIfAborted()
  const reader = new ZipReader(new BlobReader(archive))
  try {
    const entries = new Map<string, FileEntry>()
    let count = 0
    for await (const entry of reader.getEntriesGenerator()) {
      signal.throwIfAborted()
      if (++count > 256) throw new Error('В ZIP-шаблоне Concat слишком много записей')
      if (entry.directory) continue
      const path = entry.filename
      if (path.includes('\\') || path.includes('\0') ||
          path.split('/').some(part => !part || part === '.' || part === '..')) {
        throw new Error('Некорректный путь в ZIP-шаблоне Concat')
      }
      if (entries.has(path)) throw new Error('Повторный путь в ZIP-шаблоне Concat')
      entries.set(path, entry)
    }
    const manifests = [...entries.keys()].filter(path =>
      /^(?:[^/]+\/)?template\.json$/.test(path) &&
      !(entries.has('template.json') && path === 'assets/template.json'),
    )
    if (manifests.length !== 1) throw new Error('ZIP должен содержать один шаблон Concat')
    const root = manifests[0]!.slice(0, -'template.json'.length)
    let total = 0
    const bundle: [string, FileEntry][] = []
    for (const [fullPath, entry] of entries) {
      if (!fullPath.startsWith(root)) continue
      const path = fullPath.slice(root.length)
      if (path !== 'template.json' && path !== 'poster.jpg' && path !== CONCAT_SUBTITLE_SIDECAR && path !== CONCAT_CAPTION_CLIPS && !path.startsWith('assets/')) continue
      if (entry.encrypted) throw new Error('Зашифрованный ресурс шаблона')
      const limit = path === 'template.json' ? 4 * MiB : path === 'poster.jpg' ? 8 * MiB : path === CONCAT_SUBTITLE_SIDECAR ? MAX_SUBTITLE_SIDECAR_BYTES : path === CONCAT_CAPTION_CLIPS ? MAX_CONCAT_CAPTION_BYTES : MAX_CONTENT
      const size = entry.uncompressedSize
      total += size
      if (!Number.isSafeInteger(size) || size < 0 || size > limit || total > MAX_CONTENT) {
        throw new Error('Некорректный размер записи ZIP-шаблона Concat')
      }
      bundle.push([path, entry])
    }
    const files: File[] = []
    for (const [path, entry] of bundle) {
      signal.throwIfAborted()
      const chunks: BlobPart[] = []
      let written = 0
      await entry.getData(new WritableStream<Uint8Array>({
        write(chunk) {
          written += chunk.byteLength
          if (written > entry.uncompressedSize) throw new Error('Размер записи ZIP не совпадает с заявленным')
          chunks.push(new Uint8Array(chunk))
        },
      }), { checkSignature: true, signal, useWebWorkers: false })
      if (written !== entry.uncompressedSize) throw new Error('Размер записи ZIP не совпадает с заявленным')
      const file = new File(chunks, path.split('/').at(-1)!)
      Object.defineProperty(file, 'webkitRelativePath', { value: `Concat/${path}` })
      files.push(file)
    }
    return files
  } finally {
    await reader.close()
  }
}
