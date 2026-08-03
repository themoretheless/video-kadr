export type DerivedTaskKind = 'probe' | 'proxy' | 'waveform' | 'thumbnails' | 'conform'
export type DerivedTaskState = 'blocked' | 'queued' | 'running' | 'retry_wait' | 'succeeded' | 'failed' | 'cancelled' | 'permission_required'

export interface DerivedTaskInput {
  id: string
  projectId: string
  kind: DerivedTaskKind
  idempotencyKey: string
  payloadVersion: number
  payload: unknown
  priority?: number
  dependencies?: string[]
}

export interface DerivedTask extends DerivedTaskInput {
  priority: number
  priorityRevision: number
  dependencies: string[]
  consumerProjectIds: string[]
  state: DerivedTaskState
  generation: number
  attempt: number
  createdAt: number
  enqueueSequence: number
  availableAt: number
  leaseUntil?: number
  progress?: number
  result?: unknown
  error?: string
  cancelRequestedAt?: number
}

type Meta = { key: string; value: unknown }
const DB_VERSION = 2
const TASKS = 'derived_tasks'
const META = 'derived_queue_meta'
const HEAVY = new Set<DerivedTaskKind>(['proxy', 'conform'])
export const MAX_DERIVED_GRAPH_NODES = 256
export const MAX_DERIVED_GRAPH_EDGES = 1024
export const MAX_DERIVED_GRAPH_DEPTH = 32

export class DerivedPermissionRequiredError extends Error {
  constructor(message: string) { super(message); this.name = 'DerivedPermissionRequiredError' }
}

function request<T>(value: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    value.onsuccess = () => resolve(value.result)
    value.onerror = () => reject(value.error)
  })
}

function transactionDone(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve()
    tx.onabort = () => reject(tx.error ?? new Error('IndexedDB transaction aborted'))
    tx.onerror = () => reject(tx.error ?? new Error('IndexedDB transaction failed'))
  })
}

function clone<T>(value: T): T { return structuredClone(value) }

export class BrowserDerivedQueue {
  private database?: Promise<IDBDatabase>
  private readonly controllers = new Map<string, AbortController>()

  constructor(private readonly name = 'video-kadr-derived-queue', private readonly now = () => Date.now()) {}

  private open(): Promise<IDBDatabase> {
    return this.database ??= new Promise((resolve, reject) => {
      const opening = indexedDB.open(this.name, DB_VERSION)
      opening.onupgradeneeded = (event) => {
        const db = opening.result
        if (!db.objectStoreNames.contains(TASKS)) {
          const tasks = db.createObjectStore(TASKS, { keyPath: 'id' })
          tasks.createIndex('idempotency', 'idempotencyKey', { unique: true })
          tasks.createIndex('state', 'state')
        }
        if (!db.objectStoreNames.contains(META)) db.createObjectStore(META, { keyPath: 'key' })
        if (event.oldVersion < 2 && opening.transaction) {
          let sequence = 0
          opening.transaction.objectStore(TASKS).openCursor().onsuccess = event => {
            const cursor = (event.target as IDBRequest<IDBCursorWithValue | null>).result
            if (!cursor) { opening.transaction?.objectStore(META).put({ key: 'enqueueSequence', value: sequence }); return }
            const task = cursor.value as DerivedTask
            task.enqueueSequence = ++sequence; task.priorityRevision ??= 0; task.consumerProjectIds ??= [task.projectId]; cursor.update(task); cursor.continue()
          }
        }
      }
      opening.onsuccess = () => resolve(opening.result)
      opening.onerror = () => reject(opening.error)
    })
  }

  async close(): Promise<void> { (await this.open()).close(); this.database = undefined }

  async enqueueGraph(inputs: DerivedTaskInput[]): Promise<DerivedTask[]> {
    if (!inputs.length) return []
    const db = await this.open()
    if (inputs.length > MAX_DERIVED_GRAPH_NODES) throw new Error('derived graph exceeds node limit')
    const edgeCount = inputs.reduce((sum, input) => sum + (input.dependencies?.length ?? 0), 0)
    if (edgeCount > MAX_DERIVED_GRAPH_EDGES) throw new Error('derived graph exceeds edge limit')
    const tx = db.transaction([TASKS, META], 'readwrite')
    const store = tx.objectStore(TASKS)
    try {
      const projects = [...new Set(inputs.map(input => input.projectId).filter(project => project.startsWith('media:')))]
      const tombstones = await Promise.all(projects.map(project => request(tx.objectStore(META).get(`cancelledProject:${project}`))))
      if (tombstones.some(Boolean)) { await transactionDone(tx); return [] }
      const existing = await request(store.getAll()) as DerivedTask[]
      const byId = new Map(existing.map(task => [task.id, task]))
      const byKey = new Map(existing.map(task => [task.idempotencyKey, task]))
      const ids = new Set<string>()
      const keys = new Set<string>()
      for (const input of inputs) {
        if (!input.id || !input.projectId || !input.idempotencyKey || !Number.isInteger(input.payloadVersion) || input.payloadVersion < 1) throw new Error('invalid derived task')
        if (ids.has(input.id) || (byId.has(input.id) && byId.get(input.id)!.idempotencyKey !== input.idempotencyKey)) throw new Error(`duplicate task id: ${input.id}`)
        ids.add(input.id)
        if (keys.has(input.idempotencyKey)) throw new Error(`duplicate idempotency key: ${input.idempotencyKey}`)
        keys.add(input.idempotencyKey)
      }
      for (const input of inputs) if (!Number.isSafeInteger(input.priority ?? 0) || (input.priority ?? 0) < -100 || (input.priority ?? 0) > 100) throw new Error('invalid derived priority')
      const aliases = new Map(inputs.map(input => [input.id, byKey.get(input.idempotencyKey)?.id ?? input.id]))
      const sequenceRow = await request(tx.objectStore(META).get('enqueueSequence')) as Meta | undefined
      let sequence = Number(sequenceRow?.value ?? 0)
      const resolved = inputs.map(input => {
        const previous = byKey.get(input.idempotencyKey)
        const dependencies = [...new Set(input.dependencies ?? [])].map(dependency => aliases.get(dependency) ?? dependency)
        if (previous) {
          const semantic = (value: unknown) => {
            const copy = clone(value)
            if (copy && typeof copy === 'object' && !Array.isArray(copy)) delete (copy as Record<string, unknown>).mediaId
            return JSON.stringify(copy)
          }
          if (previous.kind !== input.kind || previous.payloadVersion !== input.payloadVersion || semantic(previous.payload) !== semantic(input.payload)) throw new Error('idempotency key reused with different task semantics')
          if (JSON.stringify([...previous.dependencies].sort()) !== JSON.stringify([...dependencies].sort())) throw new Error('idempotency key reused with different dependencies')
          const attached = clone(previous)
          attached.consumerProjectIds = [...new Set([...(attached.consumerProjectIds ?? [attached.projectId]), input.projectId])]
          if (!['running', 'succeeded'].includes(attached.state)) {
            attached.payload = clone(input.payload)
            if (['failed', 'cancelled'].includes(attached.state)) { attached.state = attached.dependencies.length ? 'blocked' : 'queued'; attached.attempt = 0; delete attached.error; delete attached.cancelRequestedAt }
          }
          return attached
        }
        sequence++
        return {
          ...clone(input), priority: input.priority ?? 0, dependencies,
          state: 'blocked', generation: 0, attempt: 0, priorityRevision: 0, consumerProjectIds: [input.projectId], createdAt: this.now(), enqueueSequence: sequence, availableAt: this.now(),
        } satisfies DerivedTask
      })
      const all = new Map(byId)
      for (const task of resolved) all.set(task.id, task)
      for (const task of resolved) for (const dep of task.dependencies) if (!all.has(dep)) throw new Error(`missing dependency: ${dep}`)
      const visiting = new Set<string>(), depths = new Map<string, number>()
      const visit = (id: string): number => {
        if (visiting.has(id)) throw new Error(`dependency cycle: ${id}`)
        if (depths.has(id)) return depths.get(id)!
        visiting.add(id)
        let depth = 1
        for (const dep of all.get(id)!.dependencies) depth = Math.max(depth, 1 + visit(dep))
        visiting.delete(id); depths.set(id, depth)
        if (depth > MAX_DERIVED_GRAPH_DEPTH) throw new Error('derived graph exceeds depth limit')
        return depth
      }
      for (const id of all.keys()) visit(id)
      for (const task of resolved) {
        if (byKey.has(task.idempotencyKey)) { store.put(task); continue }
        task.state = task.dependencies.length ? 'blocked' : 'queued'
        store.add(task)
      }
      tx.objectStore(META).put({ key: 'enqueueSequence', value: sequence })
      await transactionDone(tx)
      return resolved.map(clone)
    } catch (error) { tx.abort(); await transactionDone(tx).catch(() => undefined); throw error }
  }

  async list(): Promise<DerivedTask[]> {
    const db = await this.open(); return request(db.transaction(TASKS).objectStore(TASKS).getAll())
  }

  async reprioritize(id: string, priority: number, expectedRevision: number): Promise<DerivedTask> {
    if (!Number.isSafeInteger(priority) || priority < -100 || priority > 100) throw new Error('priority must be an integer from -100 to 100')
    const db = await this.open(), tx = db.transaction(TASKS, 'readwrite'), store = tx.objectStore(TASKS)
    const task = await request(store.get(id)) as DerivedTask | undefined
    if (!task) { tx.abort(); throw new Error('derived task not found') }
    if (task.state === 'running' || ['succeeded', 'cancelled'].includes(task.state)) { tx.abort(); throw new Error('only waiting tasks can be reprioritized') }
    if (task.priorityRevision !== expectedRevision) { tx.abort(); throw new Error('priority revision conflict') }
    task.priority = priority; task.priorityRevision++; store.put(task); await transactionDone(tx); return clone(task)
  }

  async heartbeat(id: string, generation: number, leaseMs = 30_000): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(TASKS, 'readwrite'), store = tx.objectStore(TASKS)
    const task = await request(store.get(id)) as DerivedTask | undefined
    if (!task || task.state !== 'running' || task.generation !== generation || task.cancelRequestedAt || (task.leaseUntil ?? 0) <= this.now()) { await transactionDone(tx); return false }
    task.leaseUntil = this.now() + Math.max(1, leaseMs); store.put(task); await transactionDone(tx); return true
  }

  async requirePermission(id: string, generation: number, reason: string): Promise<boolean> {
    return this.finish(id, generation, task => { task.state = 'permission_required'; task.error = reason; delete task.leaseUntil })
  }

  async resumePermission(id: string): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(TASKS, 'readwrite'), store = tx.objectStore(TASKS)
    const task = await request(store.get(id)) as DerivedTask | undefined
    if (!task || task.state !== 'permission_required') { await transactionDone(tx); return false }
    task.state = task.dependencies.length ? 'blocked' : 'queued'; task.availableAt = this.now(); delete task.error; store.put(task); await transactionDone(tx); return true
  }

  async retry(id: string): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(TASKS, 'readwrite'), store = tx.objectStore(TASKS)
    const task = await request(store.get(id)) as DerivedTask | undefined
    if (!task || task.state !== 'failed') { await transactionDone(tx); return false }
    task.state = task.dependencies.length ? 'blocked' : 'queued'; task.availableAt = this.now(); delete task.error; store.put(task); await transactionDone(tx); return true
  }

  async invalidateSucceeded(id: string, reason: string): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(TASKS, 'readwrite'), store = tx.objectStore(TASKS)
    const task = await request(store.get(id)) as DerivedTask | undefined
    if (!task || task.state !== 'succeeded') { await transactionDone(tx); return false }
    task.state = task.dependencies.length ? 'blocked' : 'queued'; task.generation++; task.attempt = 0
    task.availableAt = this.now(); task.error = reason; delete task.result; store.put(task)
    await transactionDone(tx); return true
  }

  async recoverExpired(): Promise<number> {
    const db = await this.open(), tx = db.transaction(TASKS, 'readwrite'), store = tx.objectStore(TASKS)
    const tasks = await request(store.getAll()) as DerivedTask[]; let count = 0; const now = this.now()
    for (const task of tasks) if (task.state === 'running' && (task.leaseUntil ?? 0) <= now) {
      task.state = task.cancelRequestedAt ? 'cancelled' : 'queued'; delete task.leaseUntil; store.put(task); count++
    }
    await transactionDone(tx); return count
  }

  async claim(leaseMs = 30_000): Promise<DerivedTask | null> {
    const db = await this.open(), tx = db.transaction([TASKS, META], 'readwrite'), store = tx.objectStore(TASKS), meta = tx.objectStore(META)
    const tasks = await request(store.getAll()) as DerivedTask[], now = this.now()
    const succeeded = new Set(tasks.filter(t => t.state === 'succeeded').map(t => t.id))
    for (const task of tasks) if (task.state === 'blocked' && task.dependencies.every(id => succeeded.has(id))) { task.state = 'queued'; delete task.error; store.put(task) }
    const runningHeavy = tasks.some(t => t.state === 'running' && HEAVY.has(t.kind) && (t.leaseUntil ?? 0) > now)
    let candidates = tasks.filter(t => (t.state === 'queued' || t.state === 'retry_wait') && t.availableAt <= now && (!HEAVY.has(t.kind) || !runningHeavy))
    if (!candidates.length) { await transactionDone(tx); return null }
    const last = await request(meta.get('lastProject')) as Meta | undefined
    const projects = [...new Set(candidates.map(t => t.projectId))].sort()
    const start = last ? (projects.indexOf(String(last.value)) + 1) % projects.length : 0
    const chosenProject = projects[start]!
    candidates = candidates.filter(t => t.projectId === chosenProject)
    const children = new Map<string, DerivedTask[]>()
    for (const task of tasks) for (const dependency of task.dependencies) children.set(dependency, [...(children.get(dependency) ?? []), task])
    const donated = (task: DerivedTask, seen = new Set<string>()): number => {
      if (seen.has(task.id)) return task.priority
      seen.add(task.id)
      return Math.max(task.priority, ...(children.get(task.id) ?? []).map(child => donated(child, new Set(seen))))
    }
    const effective = (task: DerivedTask) => donated(task) + Math.min(1000, Math.floor((now - task.createdAt) / 60_000))
    candidates.sort((a, b) => effective(b) - effective(a) || a.enqueueSequence - b.enqueueSequence || a.id.localeCompare(b.id))
    const chosen = candidates[0]!
    chosen.state = 'running'; chosen.generation++; chosen.attempt++; chosen.leaseUntil = now + Math.max(1, leaseMs)
    store.put(chosen); meta.put({ key: 'lastProject', value: chosen.projectId }); await transactionDone(tx)
    return clone(chosen)
  }

  async complete(id: string, generation: number, result: unknown): Promise<boolean> {
    return this.finish(id, generation, task => { task.state = 'succeeded'; task.result = clone(result); delete task.leaseUntil })
  }

  async fail(id: string, generation: number, error: string, retryAt?: number): Promise<boolean> {
    const finished = await this.finish(id, generation, task => { task.state = retryAt === undefined ? 'failed' : 'retry_wait'; task.error = error; task.availableAt = retryAt ?? task.availableAt; delete task.leaseUntil })
    if (finished && retryAt === undefined) await this.markDependencyBlocked(id)
    return finished
  }

  private async markDependencyBlocked(id: string): Promise<void> {
    const db = await this.open(), tx = db.transaction([TASKS, META], 'readwrite'), store = tx.objectStore(TASKS), tasks = await request(store.getAll()) as DerivedTask[]
    const affected = new Set([id]); let changed = true
    while (changed) { changed = false; for (const task of tasks) if (!affected.has(task.id) && task.dependencies.some(dep => affected.has(dep))) { affected.add(task.id); changed = true } }
    for (const task of tasks) if (task.id !== id && affected.has(task.id) && ['blocked', 'queued', 'retry_wait'].includes(task.state)) { task.state = 'blocked'; task.error = `dependency_failed:${id}`; store.put(task) }
    await transactionDone(tx)
  }

  private async finish(id: string, generation: number, update: (task: DerivedTask) => void): Promise<boolean> {
    const db = await this.open(), tx = db.transaction(TASKS, 'readwrite'), store = tx.objectStore(TASKS), task = await request(store.get(id)) as DerivedTask | undefined
    if (!task || task.state !== 'running' || task.generation !== generation || (task.leaseUntil ?? 0) <= this.now()) { await transactionDone(tx); return false }
    if (task.cancelRequestedAt) {
      task.state = 'cancelled'; delete task.leaseUntil; store.put(task); await transactionDone(tx); return false
    }
    update(task); store.put(task); await transactionDone(tx); return true
  }

  async cancel(id: string, projectId?: string): Promise<boolean> {
    const db = await this.open(), tx = db.transaction([TASKS, META], 'readwrite'), store = tx.objectStore(TASKS), tasks = await request(store.getAll()) as DerivedTask[]
    const target = tasks.find(t => t.id === id)
    if (!target) { await transactionDone(tx); return false }
    const consumers = target.consumerProjectIds ?? [target.projectId]
    const consumer = projectId ?? (consumers.length === 1 ? consumers[0] : undefined)
    if (!consumer || !consumers.includes(consumer)) { tx.abort(); throw new Error('shared derived task requires a valid project-scoped cancellation') }
    if (consumer.startsWith('media:')) tx.objectStore(META).put({ key: `cancelledProject:${consumer}`, value: this.now() })

    // A project owns its complete downstream DAG membership. Detach that membership
    // recursively; an artifact is cancelled only after its final consumer detaches.
    const detached = new Set([id]); let changed = true
    while (changed) {
      changed = false
      for (const task of tasks) if (!detached.has(task.id) && task.dependencies.some(dep => detached.has(dep)) && (task.consumerProjectIds ?? [task.projectId]).includes(consumer)) {
        detached.add(task.id); changed = true
      }
    }
    const cancelledRunning: string[] = []
    for (const task of tasks) if (detached.has(task.id)) {
      task.consumerProjectIds = (task.consumerProjectIds ?? [task.projectId]).filter(id => id !== consumer)
      if (task.consumerProjectIds.length || ['succeeded', 'failed', 'cancelled'].includes(task.state)) { store.put(task); continue }
      task.cancelRequestedAt = this.now()
      task.state = 'cancelled'
      task.generation++
      delete task.leaseUntil
      if (this.controllers.has(task.id)) cancelledRunning.push(task.id)
      store.put(task)
    }
    await transactionDone(tx)
    for (const taskId of cancelledRunning) this.controllers.get(taskId)?.abort()
    return true
  }

  async runNext(executor: (task: DerivedTask, signal: AbortSignal) => Promise<unknown>, leaseMs = 30_000): Promise<DerivedTask | null> {
    const run = async () => {
      const task = await this.claim(leaseMs); if (!task) return null
      const controller = new AbortController(); this.controllers.set(task.id, controller)
      let leaseLost = false
      const heartbeat = window.setInterval(() => void this.heartbeat(task.id, task.generation, leaseMs).then(alive => {
        if (!alive) { leaseLost = true; controller.abort() }
      }).catch(() => { leaseLost = true; controller.abort() }), Math.max(1, leaseMs / 3))
      try { await this.complete(task.id, task.generation, await executor(task, controller.signal)) }
      catch (e) {
        if (e instanceof DerivedPermissionRequiredError) await this.requirePermission(task.id, task.generation, e.message)
        else if (controller.signal.aborted || leaseLost) { /* durable cancel/recovery already owns state */ }
        else await this.fail(task.id, task.generation, e instanceof Error ? e.message : String(e))
      } finally { window.clearInterval(heartbeat); this.controllers.delete(task.id) }
      return task
    }
    const locks = navigator.locks
    return locks ? locks.request(`${this.name}:heavy`, { mode: 'exclusive' }, run) : run()
  }
}
