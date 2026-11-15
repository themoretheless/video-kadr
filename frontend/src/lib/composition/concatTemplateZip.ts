import type { FileEntry } from '@zip.js/zip.js'

const MiB = 1024 * 1024
const MAX_CONTENT = 2 * 1024 * MiB + 20 * MiB

/** Extract a single Concat bundle without writing paths to the local filesystem. */
export async function readConcatTemplateZip(archive: File, signal: AbortSignal): Promise<File[]> {
  if (archive.size > MAX_CONTENT + MiB) throw new Error('ZIP-шаблон Concat слишком большой')
  const { BlobReader, ZipReader } = await import('@zip.js/zip.js')
  signal.throwIfAborted()
  const reader = new ZipReader(new BlobReader(archive))
  try {
    const entries = new Map<string, FileEntry>()
    let total = 0
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
      if (path !== 'template.json' && path !== 'poster.jpg' && !path.startsWith('assets/')) continue
      if (entry.encrypted || entries.has(path)) throw new Error('Повторный или зашифрованный ресурс шаблона')
      const limit = path === 'template.json' ? 4 * MiB : path === 'poster.jpg' ? 8 * MiB : MAX_CONTENT
      const size = entry.uncompressedSize
      total += size
      if (!Number.isSafeInteger(size) || size < 0 || size > limit || total > MAX_CONTENT) {
        throw new Error('Некорректный размер записи ZIP-шаблона Concat')
      }
      entries.set(path, entry)
    }
    if (!entries.has('template.json')) throw new Error('В корне ZIP-шаблона отсутствует template.json')
    const files: File[] = []
    for (const [path, entry] of entries) {
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
