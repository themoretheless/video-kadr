import { validateExportJobDefinition, type ExportJobDefinitionV1 } from './domain/export-variants'

export type ExportQueueState = 'queued' | 'running' | 'interrupted' | 'succeeded' | 'failed' | 'cancelled' | 'permission_required'
export interface ExportQueueJob {
  definition: ExportJobDefinitionV1
  state: ExportQueueState
  generation: number
  attempt: number
  enqueueSequence: number
  createdAt: number
  leaseUntil?: number
  progress?: number
  stage?: string
  error?: string
  recoveryKind?: 'source' | 'lut'
  result?: Record<string, unknown>
}

const STORE = 'export_jobs'; const META = 'export_meta'; const VERSION = 1
type Meta = { key: string; value: number }
const request = <T>(value: IDBRequest<T>) => new Promise<T>((resolve, reject) => { value.onsuccess = () => resolve(value.result); value.onerror = () => reject(value.error) })
const done = (tx: IDBTransaction) => new Promise<void>((resolve, reject) => { tx.oncomplete = () => resolve(); tx.onabort = () => reject(tx.error); tx.onerror = () => reject(tx.error) })

export class BrowserExportQueue {
  private database?: Promise<IDBDatabase>
  constructor(private readonly name = 'video-kadr-export-queue', private readonly now = () => Date.now()) {}
  private open(): Promise<IDBDatabase> {
    return this.database ??= new Promise((resolve, reject) => {
      const opening = indexedDB.open(this.name, VERSION)
      opening.onupgradeneeded = () => {
        const db = opening.result
        if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE, { keyPath: 'definition.id' })
        if (!db.objectStoreNames.contains(META)) db.createObjectStore(META, { keyPath: 'key' })
      }
      opening.onsuccess = () => resolve(opening.result); opening.onerror = () => reject(opening.error)
    })
  }
  async close(): Promise<void> { (await this.open()).close(); this.database = undefined }
  async enqueue(definitions: readonly ExportJobDefinitionV1[]): Promise<ExportQueueJob[]> {
    if (!definitions.length) return []
    const db = await this.open(), tx = db.transaction([STORE, META], 'readwrite'), store = tx.objectStore(STORE), meta = tx.objectStore(META)
    try {
      const existing = await request(store.getAll()) as ExportQueueJob[]
      const ids = new Set(existing.map(job => job.definition.id)); const inputIds = new Set<string>()
      const row = await request(meta.get('sequence')) as Meta | undefined; let sequence = row?.value ?? 0
      const jobs = definitions.map(rawDefinition => {
        const definition = validateExportJobDefinition(rawDefinition)
        if (ids.has(definition.id) || inputIds.has(definition.id) || definition.contract !== 'export-batch-v1') throw new Error(`duplicate or invalid export definition: ${definition.id}`)
        inputIds.add(definition.id)
        return { definition: structuredClone(definition), state: 'queued', generation: 0, attempt: 0, enqueueSequence: ++sequence, createdAt: this.now() } satisfies ExportQueueJob
      })
      for (const job of jobs) store.add(job)
      meta.put({ key: 'sequence', value: sequence }); await done(tx); return structuredClone(jobs)
    } catch (error) { tx.abort(); await done(tx).catch(() => undefined); throw error }
  }
  async list(): Promise<ExportQueueJob[]> { const db = await this.open(); return structuredClone(await request(db.transaction(STORE).objectStore(STORE).getAll()) as ExportQueueJob[]).sort((a, b) => a.enqueueSequence - b.enqueueSequence) }
  async claim(leaseMs = 30_000): Promise<ExportQueueJob | null> {
    const db = await this.open(), tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE), jobs = await request(store.getAll()) as ExportQueueJob[], now = this.now()
    if (jobs.some(job => job.state === 'running')) { await done(tx); return null }
    const job = jobs.filter(item => item.state === 'queued').sort((a, b) => a.enqueueSequence - b.enqueueSequence)[0]
    if (!job) { await done(tx); return null }
    job.state = 'running'; job.generation++; job.attempt++; job.leaseUntil = now + Math.max(1, leaseMs); delete job.error; delete job.recoveryKind; delete job.result; delete job.progress; delete job.stage
    store.put(job); await done(tx); return structuredClone(job)
  }
  private async finish(id: string, generation: number, update: (job: ExportQueueJob) => void): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE), job = await request(store.get(id)) as ExportQueueJob | undefined
    if (!job || job.state !== 'running' || job.generation !== generation || (job.leaseUntil ?? 0) <= this.now()) { await done(tx); return false }
    update(job); delete job.leaseUntil; store.put(job); await done(tx); return true
  }
  complete(id: string, generation: number, result: Record<string, unknown>): Promise<boolean> { return this.finish(id, generation, job => { job.state = 'succeeded'; job.result = structuredClone(result); job.progress = 100 }) }
  fail(id: string, generation: number, error: string): Promise<boolean> { return this.finish(id, generation, job => { job.state = 'failed'; job.error = error }) }
  requirePermission(id: string, generation: number, error: string, recoveryKind: 'source' | 'lut'): Promise<boolean> { return this.finish(id, generation, job => { job.state = 'permission_required'; job.error = error; job.recoveryKind = recoveryKind }) }
  async heartbeat(id: string, generation: number, leaseMs = 30_000): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE), job = await request(store.get(id)) as ExportQueueJob | undefined
    if (!job || job.state !== 'running' || job.generation !== generation || (job.leaseUntil ?? 0) <= this.now()) { await done(tx); return false }
    job.leaseUntil = this.now() + Math.max(1, leaseMs); store.put(job); await done(tx); return true
  }
  async progress(id: string, generation: number, progress: number, stage: string): Promise<boolean> {
    if (!Number.isFinite(progress) || progress < 0 || progress > 100 || typeof stage !== 'string' || stage.length > 240) throw new Error('invalid export progress')
    const db = await this.open(), tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE), job = await request(store.get(id)) as ExportQueueJob | undefined
    if (!job || job.state !== 'running' || job.generation !== generation || (job.leaseUntil ?? 0) <= this.now()) { await done(tx); return false }
    job.progress = progress; job.stage = stage; store.put(job); await done(tx); return true
  }
  async recoverRunning(): Promise<number> {
    const db = await this.open(), tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE), jobs = await request(store.getAll()) as ExportQueueJob[]; let count = 0
    const now = this.now()
    for (const job of jobs) {
      const expiredRun = job.state === 'running' && (job.leaseUntil ?? 0) <= now
      const durableResult = typeof job.result?.durableAssetRef === 'string' && job.result.durableAssetRef.length > 0
        || typeof job.result?.opfsPath === 'string' && job.result.opfsPath.length > 0
      const staleCompletion = job.state === 'succeeded' && !durableResult
      if (!expiredRun && !staleCompletion) continue
      job.state = 'interrupted'; job.generation++
      job.error = staleCompletion
        ? 'Completed browser output was session-only; export must restart from zero'
        : 'WASM process interrupted; export will restart from zero'
      delete job.leaseUntil; delete job.progress; delete job.stage; delete job.result
      store.put(job); count++
    }
    await done(tx); return count
  }
  async retry(id: string): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE), job = await request(store.get(id)) as ExportQueueJob | undefined
    if (!job || !['failed', 'interrupted', 'permission_required', 'cancelled'].includes(job.state)) { await done(tx); return false }
    job.state = 'queued'; delete job.error; delete job.recoveryKind; delete job.result; delete job.progress; delete job.stage; store.put(job); await done(tx); return true
  }
  async cancel(id: string): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE), job = await request(store.get(id)) as ExportQueueJob | undefined
    if (!job || ['succeeded', 'cancelled'].includes(job.state)) { await done(tx); return false }
    job.state = 'cancelled'; job.generation++; delete job.leaseUntil; delete job.recoveryKind; store.put(job); await done(tx); return true
  }
}
