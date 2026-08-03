import { CaptureSessionError } from './domain/capture-session'

const MEMORY_FALLBACK_LIMIT = 128 * 1024 * 1024
const MAX_PENDING_BYTES = 8 * 1024 * 1024
const STALE_STAGE_MS = 24 * 60 * 60 * 1000

type StorageWithDirectory = StorageManager & { getDirectory?: () => Promise<FileSystemDirectoryHandle> }

export class BrowserCaptureSink {
  onFailure: ((error: CaptureSessionError) => void) | null = null
  private chunks: Blob[] = []
  private bytes = 0
  private pendingBytes = 0
  private writes: Promise<void> = Promise.resolve()
  private failed: unknown = null
  private directory: FileSystemDirectoryHandle | null = null
  private handle: FileSystemFileHandle | null = null
  private writable: FileSystemWritableFileStream | null = null
  private temporaryName = ''
  private closed = false

  static async create(): Promise<BrowserCaptureSink> {
    const sink = new BrowserCaptureSink()
    const storage = typeof navigator === 'undefined' ? undefined : navigator.storage as StorageWithDirectory | undefined
    if (!storage?.getDirectory) return sink
    try {
      const root = await storage.getDirectory()
      sink.directory = await root.getDirectoryHandle('capture-staging-v1', { create: true })
      const entries = (sink.directory as unknown as { entries?: () => AsyncIterableIterator<[string, FileSystemHandle]> }).entries
      if (entries) {
        for await (const [name, entry] of entries.call(sink.directory)) {
          if (!name.endsWith('.webm.part') || entry.kind !== 'file') continue
          const file = await (entry as FileSystemFileHandle).getFile()
          if (Date.now() - file.lastModified > STALE_STAGE_MS) await sink.directory.removeEntry(name).catch(() => undefined)
        }
      }
      sink.temporaryName = `capture-${crypto.randomUUID()}.webm.part`
      sink.handle = await sink.directory.getFileHandle(sink.temporaryName, { create: true })
      sink.writable = await sink.handle.createWritable({ keepExistingData: false })
    } catch {
      sink.directory = null; sink.handle = null; sink.writable = null; sink.temporaryName = ''
    }
    return sink
  }

  get size(): number { return this.bytes }
  get streaming(): boolean { return this.writable !== null }

  private fail(error: CaptureSessionError): void {
    if (this.failed) return
    this.failed = error
    this.onFailure?.(error)
  }

  append(chunk: Blob): void {
    if (this.closed || this.failed || chunk.size === 0) return
    this.bytes += chunk.size
    if (!this.writable) {
      if (this.bytes > MEMORY_FALLBACK_LIMIT) {
        this.fail(new CaptureSessionError('limit_reached', 'Запись превысила лимит памяти; OPFS недоступен'))
        this.chunks = []
        return
      }
      this.chunks.push(chunk)
      return
    }
    this.pendingBytes += chunk.size
    if (this.pendingBytes > MAX_PENDING_BYTES) {
      this.fail(new CaptureSessionError('storage_failed', 'Хранилище не успевает записывать видео'))
      return
    }
    this.writes = this.writes.then(async () => {
      if (this.failed || !this.writable) return
      try { await this.writable.write(chunk) }
      catch (error) { this.fail(new CaptureSessionError('storage_failed', 'Не удалось записать чанк в OPFS', { cause: error })) }
      finally { this.pendingBytes -= chunk.size }
    })
  }

  async finish(filename: string, mimeType: string): Promise<File> {
    if (this.closed) throw new CaptureSessionError('storage_failed', 'Хранилище записи уже закрыто')
    this.closed = true
    await this.writes
    if (this.failed) { await this.abort(); throw this.failed }
    if (this.writable && this.handle) {
      await this.writable.close().catch(error => { throw new CaptureSessionError('storage_failed', 'Не удалось завершить OPFS-файл', { cause: error }) })
      this.writable = null
      const staged = await this.handle.getFile()
      return new File([staged], filename, { type: mimeType, lastModified: Date.now() })
    }
    return new File(this.chunks, filename, { type: mimeType, lastModified: Date.now() })
  }

  async abort(): Promise<void> {
    this.closed = true
    await this.writable?.abort().catch(() => undefined)
    this.writable = null
    this.chunks = []
    if (this.directory && this.temporaryName) await this.directory.removeEntry(this.temporaryName).catch(() => undefined)
  }

  async removeStage(): Promise<void> {
    if (this.directory && this.temporaryName) await this.directory.removeEntry(this.temporaryName).catch(() => undefined)
  }
}
