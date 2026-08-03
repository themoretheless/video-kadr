import type { ProjectDto } from './api'
import { migrateProjectDocument } from './project-schema'
import { sha256 } from '@noble/hashes/sha2.js'

const DATABASE = 'video-kadr'
const VERSION = 2
const STORE = 'projects'
const SNAPSHOTS = 'project-snapshots'
const HEADS = 'project-heads'
const JOURNALS = 'project-journals'
const RETAIN_SNAPSHOTS = 5
const WRITER_ID = crypto.randomUUID()
let lastJournalSequence = 0
const latestWriterSequence = new Map<string, number>()

interface ProjectSnapshot { key: string; projectId: string; revision: number; checksum: string; project: ProjectDto; createdAt: number }
interface ProjectHead { projectId: string; videoId: string; headRevision: number; previousRevision?: number }
interface ProjectJournal { id: string; writerId: string; sequence: number; projectId: string; videoId: string; expectedRevision: number; candidateRevision: number; candidate: ProjectDto; candidateChecksum: string; checksum: string; phase: 'prepared'; createdAt: number }
export interface ProjectRecoveryCandidate { projectId: string; videoId: string; corruptRevision: number; candidate: ProjectDto | null; candidateRevision: number | null; journal: boolean; journalId?: string; writerId?: string; journalSequence?: number; reason: 'draft' | 'corrupt' }

function nextJournalSequence(): number {
  lastJournalSequence = Math.max(lastJournalSequence + 1, Date.now() * 1000)
  return lastJournalSequence
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, VERSION)
    request.onupgradeneeded = () => {
      const database = request.result
      if (!database.objectStoreNames.contains(STORE)) {
        const store = database.createObjectStore(STORE, { keyPath: 'id' })
        store.createIndex('videoId', 'videoId', { unique: false })
        store.createIndex('updatedAt', 'updatedAt', { unique: false })
      }
      if (!database.objectStoreNames.contains(SNAPSHOTS)) {
        const snapshots = database.createObjectStore(SNAPSHOTS, { keyPath: 'key' })
        snapshots.createIndex('projectId', 'projectId', { unique: false })
      }
      if (!database.objectStoreNames.contains(HEADS)) database.createObjectStore(HEADS, { keyPath: 'projectId' })
      if (!database.objectStoreNames.contains(JOURNALS)) {
        const journals = database.createObjectStore(JOURNALS, { keyPath: 'id' })
        journals.createIndex('projectId', 'projectId', { unique: false })
      }
    }
    request.onsuccess = () => {
      const database = request.result
      backfillLegacyProjects(database).then(() => resolve(database), reject)
    }
    request.onerror = () => reject(request.error ?? new Error('Не удалось открыть IndexedDB'))
  })
}

async function backfillLegacyProjects(database: IDBDatabase): Promise<void> {
  const projects = await requestResult<ProjectDto[]>(database.transaction(STORE).objectStore(STORE).getAll())
  const heads = await requestResult<ProjectHead[]>(database.transaction(HEADS).objectStore(HEADS).getAll())
  const known = new Set(heads.map((head) => head.projectId))
  const missing: Array<{ project: ProjectDto; checksum: string }> = []
  for (const value of projects) {
    if (known.has(value.id)) continue
    try {
      const project = validateProject(value)
      missing.push({ project, checksum: await checksum(project) })
    } catch { /* quarantine a corrupt legacy row without hiding healthy projects */ }
  }
  if (!missing.length) return
  const transaction = database.transaction([SNAPSHOTS, HEADS], 'readwrite')
  const committed = transactionDone(transaction)
  for (const item of missing) {
    const revision = item.project.revision!
    transaction.objectStore(SNAPSHOTS).put({ key: snapshotKey(item.project.id, revision), projectId: item.project.id, revision, checksum: item.checksum, project: item.project, createdAt: Date.now() } satisfies ProjectSnapshot)
    transaction.objectStore(HEADS).put({ projectId: item.project.id, videoId: item.project.videoId, headRevision: revision } satisfies ProjectHead)
  }
  await committed
}

function snapshotKey(projectId: string, revision: number): string { return `${projectId}:${revision}` }

function validateProject(project: ProjectDto): ProjectDto {
  const clone = structuredClone(project)
  if (!clone.id || !clone.videoId || !Number.isSafeInteger(clone.revision) || (clone.revision ?? 0) < 1) {
    throw new Error('Повреждённая запись проекта')
  }
  if (clone.document) clone.document = migrateProjectDocument(clone.document)
  return clone
}

function checksum(project: ProjectDto): string {
  return digest(project)
}

function digest(value: unknown): string {
  const canonical = (item: unknown): unknown => {
    if (Array.isArray(item)) return item.map(canonical)
    if (item && typeof item === 'object') return Object.fromEntries(
      Object.entries(item as Record<string, unknown>).sort(([left], [right]) => left.localeCompare(right)).map(([key, nested]) => [key, canonical(nested)]),
    )
    return item
  }
  const bytes = new TextEncoder().encode(JSON.stringify(canonical(value)))
  return [...sha256(bytes)].map((byte) => byte.toString(16).padStart(2, '0')).join('')
}

function journalChecksum(journal: Omit<ProjectJournal, 'checksum'>): string { return digest(journal) }

async function validJournal(journal: ProjectJournal): Promise<ProjectDto | null> {
  const { checksum: integrity, ...envelope } = journal
  if (journalChecksum(envelope) !== integrity) return null
  return validSnapshot({ key: '', projectId: journal.projectId, revision: journal.candidateRevision, checksum: journal.candidateChecksum, project: journal.candidate, createdAt: journal.createdAt })
}

export function getProjectDraftWatermark(projectId: string): number {
  return latestWriterSequence.get(projectId) ?? 0
}

async function validSnapshot(snapshot: ProjectSnapshot | undefined): Promise<ProjectDto | null> {
  if (!snapshot) return null
  try {
    if (checksum(snapshot.project) !== snapshot.checksum) return null
    const project = validateProject(snapshot.project)
    if (
      project.id !== snapshot.projectId
      || project.revision !== snapshot.revision
      || (snapshot.key && snapshot.key !== snapshotKey(snapshot.projectId, snapshot.revision))
    ) return null
    return project
  } catch { return null }
}

async function snapshotsFor(database: IDBDatabase, projectId: string): Promise<ProjectSnapshot[]> {
  const values = await requestResult<ProjectSnapshot[]>(
    database.transaction(SNAPSHOTS, 'readonly').objectStore(SNAPSHOTS).index('projectId').getAll(projectId),
  )
  return values.sort((a, b) => b.revision - a.revision)
}

async function journalsFor(database: IDBDatabase, projectId: string): Promise<ProjectJournal[]> {
  const values = await requestResult<ProjectJournal[]>(
    database.transaction(JOURNALS, 'readonly').objectStore(JOURNALS).index('projectId').getAll(projectId),
  )
  return values.sort((a, b) => (b.sequence ?? b.createdAt * 1000) - (a.sequence ?? a.createdAt * 1000) || b.id.localeCompare(a.id))
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('Ошибка IndexedDB'))
  })
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error ?? new Error('Ошибка IndexedDB'))
    transaction.onabort = () => reject(transaction.error ?? new Error('Транзакция IndexedDB отменена'))
  })
}

export async function putProject(project: ProjectDto): Promise<void> {
  const validated = validateProject(project)
  const encoded = await checksum(validated)
  const database = await openDatabase()
  try {
    const key = snapshotKey(validated.id, validated.revision!)
    const existingSnapshot = await requestResult<ProjectSnapshot | undefined>(
      database.transaction(SNAPSHOTS).objectStore(SNAPSHOTS).get(key),
    )
    const existingHead = await requestResult<ProjectHead | undefined>(database.transaction(HEADS).objectStore(HEADS).get(validated.id))
    if (existingHead && validated.revision! < existingHead.headRevision) throw new Error('Project revision cannot move backwards')
    const transaction = database.transaction([STORE, SNAPSHOTS, HEADS], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(STORE).put(validated)
    if (!existingSnapshot) transaction.objectStore(SNAPSHOTS).add({ key, projectId: validated.id, revision: validated.revision!, checksum: encoded, project: validated, createdAt: Date.now() } satisfies ProjectSnapshot)
    transaction.objectStore(HEADS).put({ projectId: validated.id, videoId: validated.videoId, headRevision: validated.revision! } satisfies ProjectHead)
    await committed
  } finally { database.close() }
}

export async function prepareProjectDraft(project: ProjectDto, expectedRevision: number): Promise<number> {
  const candidate = validateProject(project)
  if (candidate.revision !== expectedRevision + 1) throw new Error('Draft revision must follow the committed head')
  const sequence = nextJournalSequence()
  const createdAt = Date.now()
  const encoded = await checksum(candidate)
  const database = await openDatabase()
  try {
    const transaction = database.transaction(JOURNALS, 'readwrite')
    const committed = transactionDone(transaction)
    const envelope = {
      id: crypto.randomUUID(), writerId: WRITER_ID, projectId: candidate.id, videoId: candidate.videoId,
      expectedRevision, candidateRevision: candidate.revision, candidate,
      candidateChecksum: encoded, phase: 'prepared' as const, createdAt, sequence,
    }
    transaction.objectStore(JOURNALS).put({ ...envelope, checksum: journalChecksum(envelope) } satisfies ProjectJournal)
    await committed
    latestWriterSequence.set(candidate.id, Math.max(latestWriterSequence.get(candidate.id) ?? 0, sequence))
    return sequence
  } finally { database.close() }
}

/** One read/write transaction prevents two tabs from accepting the same revision. */
export async function compareAndSwapProject(
  videoId: string,
  projectId: string | undefined,
  expectedRevision: number | undefined,
  build: (previous: ProjectDto | undefined) => ProjectDto,
  conflict: () => Error,
  writerWatermark = 0,
): Promise<ProjectDto> {
  const observed = projectId ? await projectById(projectId) : await projectByVideo(videoId)
  if (expectedRevision !== undefined && (observed?.revision ?? 0) !== expectedRevision) throw conflict()
  const candidate = validateProject(build(observed ?? undefined))
  if ((projectId && candidate.id !== projectId) || candidate.videoId !== videoId) {
    throw new Error('Candidate project identity does not match CAS target')
  }
  if (candidate.revision !== (expectedRevision ?? (observed?.revision ?? 0)) + 1) throw new Error('Project revision must increase by exactly one')
  const commitExpectedRevision = expectedRevision ?? (observed?.revision ?? 0)
  const encoded = await checksum(candidate)
  const journalId = crypto.randomUUID()
  const journalSequence = nextJournalSequence()
  const prepared = await openDatabase()
  try {
    const transaction = prepared.transaction(JOURNALS, 'readwrite')
    const committed = transactionDone(transaction)
    const envelope = { id: journalId, writerId: WRITER_ID, sequence: journalSequence, projectId: candidate.id, videoId, expectedRevision: commitExpectedRevision, candidateRevision: candidate.revision!, candidate, candidateChecksum: encoded, phase: 'prepared' as const, createdAt: Date.now() }
    transaction.objectStore(JOURNALS).put({ ...envelope, checksum: journalChecksum(envelope) } satisfies ProjectJournal)
    await committed
  } finally { prepared.close() }
  const database = await openDatabase()
  try {
    const preparedJournals = await journalsFor(database, candidate.id)
    return await new Promise<ProjectDto>((resolve, reject) => {
      const transaction = database.transaction([STORE, SNAPSHOTS, HEADS, JOURNALS], 'readwrite')
      const store = transaction.objectStore(STORE)
      const read = (projectId ? store.get(projectId) : store.index('videoId').get(videoId)) as IDBRequest<ProjectDto | undefined>
      let next: ProjectDto | undefined
      read.onerror = () => reject(read.error ?? new Error('Ошибка IndexedDB'))
      read.onsuccess = () => {
        const previous = read.result
        if (
          (!previous && commitExpectedRevision !== 0) ||
          (previous && (previous.revision ?? 0) !== commitExpectedRevision)
        ) {
          transaction.abort()
          reject(conflict())
          return
        }
        try {
          next = candidate
          const revision = candidate.revision!
          store.put(candidate)
          transaction.objectStore(SNAPSHOTS).add({ key: snapshotKey(candidate.id, revision), projectId: candidate.id, revision, checksum: encoded, project: candidate, createdAt: Date.now() } satisfies ProjectSnapshot)
          transaction.objectStore(HEADS).put({ projectId: candidate.id, videoId, headRevision: revision, previousRevision: previous?.revision } satisfies ProjectHead)
          for (const journal of preparedJournals) {
            if (journal.id === journalId || (
              journal.writerId === WRITER_ID && (journal.sequence ?? 0) <= writerWatermark
            )) {
              transaction.objectStore(JOURNALS).delete(journal.id)
            }
          }
        } catch (error) {
          transaction.abort()
          reject(error)
        }
      }
      transaction.oncomplete = () => {
        if (next) resolve(next)
      }
      transaction.onerror = () => reject(transaction.error ?? new Error('Ошибка IndexedDB'))
      transaction.onabort = () => {
        if (transaction.error) reject(transaction.error)
      }
    }).then(async (saved) => { try { await pruneSnapshots(saved.id) } catch { /* commit is already durable */ } return saved })
  } finally {
    database.close()
  }
}

async function projectById(projectId: string): Promise<ProjectDto | null> {
  const database = await openDatabase()
  try {
    const project = await requestResult<ProjectDto | undefined>(database.transaction(STORE).objectStore(STORE).get(projectId))
    return project ? validateProject(project) : null
  }
  finally { database.close() }
}

async function pruneSnapshots(projectId: string): Promise<void> {
  const database = await openDatabase()
  try {
    const snapshots = await snapshotsFor(database, projectId)
    if (snapshots.length <= RETAIN_SNAPSHOTS) return
    const transaction = database.transaction(SNAPSHOTS, 'readwrite'); const committed = transactionDone(transaction)
    for (const snapshot of snapshots.slice(RETAIN_SNAPSHOTS)) transaction.objectStore(SNAPSHOTS).delete(snapshot.key)
    await committed
  } finally { database.close() }
}

export async function inspectProjectRecovery(projectId: string): Promise<ProjectRecoveryCandidate | null> {
  const database = await openDatabase()
  try {
    const rawHeadRecord = await requestResult<ProjectHead | undefined>(database.transaction(HEADS).objectStore(HEADS).get(projectId))
    const head = rawHeadRecord
      && rawHeadRecord.projectId === projectId
      && Number.isSafeInteger(rawHeadRecord.headRevision)
      && rawHeadRecord.headRevision >= 1
      ? rawHeadRecord
      : undefined
    const rawHead = await requestResult<ProjectDto | undefined>(database.transaction(STORE).objectStore(STORE).get(projectId))
    const journals = await journalsFor(database, projectId)
    let journal: ProjectJournal | undefined
    for (const item of journals) {
      const draft = await validJournal(item)
      if (draft) { journal = item; break }
    }
    if (!head && !journal && !rawHead) return null
    const snapshots = await snapshotsFor(database, projectId)
    if (!head && !journal && rawHead) {
      for (const snapshot of snapshots) {
        const candidate = await validSnapshot(snapshot)
        if (candidate) return { projectId, videoId: candidate.videoId, corruptRevision: 0, candidate, candidateRevision: snapshot.revision, journal: false, reason: 'corrupt' }
      }
      return {
        projectId,
        videoId: typeof rawHead.videoId === 'string' ? rawHead.videoId : '',
        corruptRevision: Number.isSafeInteger(rawHead.revision) ? rawHead.revision! : 0,
        candidate: null,
        candidateRevision: null,
        journal: false,
        reason: 'corrupt',
      }
    }
    const current = snapshots.find((item) => item.revision === head?.headRevision)
    const currentProject = await validSnapshot(current)
    let rawHeadValid = false
    if (rawHead && current) {
      try {
        const validated = validateProject(rawHead)
        rawHeadValid = checksum(validated) === current.checksum
      } catch { rawHeadValid = false }
    }
    if (journal) {
      const draft = await validJournal(journal)
      if (draft) return { projectId, videoId: journal.videoId, corruptRevision: head?.headRevision ?? 0, candidate: draft, candidateRevision: journal.candidateRevision, journal: true, journalId: journal.id, writerId: journal.writerId, journalSequence: journal.sequence, reason: 'draft' }
    }
    if (currentProject && rawHeadValid && !journal) return null
    if (currentProject && !rawHeadValid && head) {
      return {
        projectId, videoId: head.videoId, corruptRevision: head.headRevision,
        candidate: currentProject, candidateRevision: current!.revision,
        journal: Boolean(journal), reason: 'corrupt',
      }
    }
    if (currentProject && journal) {
      return { projectId, videoId: journal.videoId, corruptRevision: journal.candidateRevision, candidate: currentProject, candidateRevision: current!.revision, journal: true, reason: 'draft' }
    }
    for (const snapshot of snapshots.filter((item) => item.revision !== head?.headRevision)) {
      const candidate = await validSnapshot(snapshot)
      if (candidate) return { projectId, videoId: head?.videoId ?? journal!.videoId, corruptRevision: head?.headRevision ?? journal!.candidateRevision, candidate, candidateRevision: snapshot.revision, journal: Boolean(journal), reason: 'corrupt' }
    }
    return { projectId, videoId: head?.videoId ?? journal!.videoId, corruptRevision: head?.headRevision ?? journal!.candidateRevision, candidate: null, candidateRevision: null, journal: Boolean(journal), reason: 'corrupt' }
  } finally { database.close() }
}

export async function inspectProjectRecoveryByVideo(videoId: string): Promise<ProjectRecoveryCandidate | null> {
  const database = await openDatabase()
  try {
    const project = await requestResult<ProjectDto | undefined>(
      database.transaction(STORE).objectStore(STORE).index('videoId').get(videoId),
    )
    if (project?.id) return inspectProjectRecovery(project.id)
    const journals = await requestResult<ProjectJournal[]>(
      database.transaction(JOURNALS).objectStore(JOURNALS).index('projectId').getAll(),
    )
    const journal = journals.sort((a, b) => (b.sequence ?? b.createdAt * 1000) - (a.sequence ?? a.createdAt * 1000) || b.id.localeCompare(a.id)).find((item) => item.videoId === videoId)
    return journal ? inspectProjectRecovery(journal.projectId) : null
  } finally { database.close() }
}

export async function recoverProject(projectId: string, expectedHeadRevision: number, candidateRevision: number, journalId?: string): Promise<ProjectDto> {
  const database = await openDatabase()
  try {
    const journals = await journalsFor(database, projectId)
    const journal = journalId ? journals.find((item) => item.id === journalId) : undefined
    if (journalId && !journal) throw new Error('Recovery conflict')
    const verifiedJournalCandidate = journal ? await validJournal(journal) : null
    if (journal && !verifiedJournalCandidate) throw new Error('Recovery conflict')
    const snapshot = journal?.candidateRevision === candidateRevision && verifiedJournalCandidate
      ? { key: '', projectId, revision: journal.candidateRevision, checksum: checksum(verifiedJournalCandidate), project: verifiedJournalCandidate, createdAt: journal.createdAt }
      : await requestResult<ProjectSnapshot | undefined>(database.transaction(SNAPSHOTS).objectStore(SNAPSHOTS).get(snapshotKey(projectId, candidateRevision)))
    const source = await validSnapshot(snapshot); if (!source) throw new Error('Recovery snapshot повреждён')
    const snapshots = await snapshotsFor(database, projectId)
    const validSnapshotRevisions: number[] = []
    for (const item of snapshots) if (await validSnapshot(item)) validSnapshotRevisions.push(item.revision)
    const maxPersistedRevision = Math.max(expectedHeadRevision, 0, ...validSnapshotRevisions)
    const candidate = source.revision! > maxPersistedRevision
      ? source
      : validateProject({ ...source, revision: maxPersistedRevision + 1, updatedAt: Date.now() })
    const encoded = await checksum(candidate)
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction([STORE, SNAPSHOTS, HEADS, JOURNALS], 'readwrite')
      const read = transaction.objectStore(HEADS).get(projectId) as IDBRequest<ProjectHead | undefined>
      let conflict = false
      read.onerror = () => reject(read.error ?? new Error('Ошибка IndexedDB'))
      read.onsuccess = () => {
        const rawHead = read.result
        const head = rawHead && Number.isSafeInteger(rawHead.headRevision) && rawHead.headRevision >= 1 ? rawHead : undefined
        if ((!head && expectedHeadRevision !== 0) || (head && head.headRevision !== expectedHeadRevision)) {
          conflict = true
          transaction.abort()
          return
        }
        transaction.objectStore(STORE).put(candidate)
        transaction.objectStore(SNAPSHOTS).add({ key: snapshotKey(projectId, candidate.revision!), projectId, revision: candidate.revision!, checksum: encoded, project: candidate, createdAt: Date.now() } satisfies ProjectSnapshot)
        transaction.objectStore(HEADS).put(head
          ? { ...head, headRevision: candidate.revision!, previousRevision: head.headRevision }
          : { projectId, videoId: candidate.videoId, headRevision: candidate.revision! } satisfies ProjectHead)
        if (journal) {
          for (const item of journals) {
            if (item.writerId === journal.writerId && (item.sequence ?? 0) <= (journal.sequence ?? 0)) transaction.objectStore(JOURNALS).delete(item.id)
          }
        }
      }
      transaction.oncomplete = () => resolve()
      transaction.onerror = () => reject(transaction.error ?? new Error('Ошибка IndexedDB'))
      transaction.onabort = () => reject(conflict ? new Error('Recovery conflict') : (transaction.error ?? new Error('Транзакция IndexedDB отменена')))
    })
    try { await pruneSnapshots(projectId) } catch { /* recovery commit is already durable */ }
    return candidate
  } finally { database.close() }
}

export async function discardProjectRecovery(projectId: string, journalId: string): Promise<void> {
  const database = await openDatabase()
  try {
    const journals = await journalsFor(database, projectId)
    const selected = journals.find((item) => item.id === journalId)
    if (!selected) return
    if (!await validJournal(selected)) throw new Error('Recovery conflict')
    const transaction = database.transaction(JOURNALS, 'readwrite'); const committed = transactionDone(transaction)
    for (const journal of journals) {
      if (journal.writerId === selected.writerId && (journal.sequence ?? 0) <= (selected.sequence ?? 0)) transaction.objectStore(JOURNALS).delete(journal.id)
    }
    await committed
  }
  finally { database.close() }
}

export async function projectByVideo(videoId: string): Promise<ProjectDto | null> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(STORE, 'readonly')
    const result = await requestResult<ProjectDto | undefined>(
      transaction.objectStore(STORE).index('videoId').get(videoId),
    )
    return result ? validateProject(result) : null
  } finally {
    database.close()
  }
}

export async function allProjects(): Promise<ProjectDto[]> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(STORE, 'readonly')
    const projects = await requestResult<ProjectDto[]>(transaction.objectStore(STORE).getAll())
    const safe: ProjectDto[] = []
    for (const project of projects) {
      const snapshots = await snapshotsFor(database, project.id)
      const head = await requestResult<ProjectHead | undefined>(database.transaction(HEADS).objectStore(HEADS).get(project.id))
      try {
        const validated = validateProject(project)
        const current = snapshots.find((snapshot) => snapshot.revision === head?.headRevision)
        if ((!head && !current) || (current && checksum(validated) === current.checksum)) {
          safe.push(validated)
          continue
        }
      } catch { /* recover from an immutable snapshot below */ }
      for (const snapshot of snapshots) {
        const recovered = await validSnapshot(snapshot)
        if (recovered) { safe.push(recovered); break }
      }
    }
    return safe.sort((left, right) => right.updatedAt - left.updatedAt)
  } finally {
    database.close()
  }
}

export async function removeProject(projectId: string): Promise<void> {
  const database = await openDatabase()
  try {
    const snapshots = await snapshotsFor(database, projectId)
    const journals = await journalsFor(database, projectId)
    const transaction = database.transaction([STORE, SNAPSHOTS, HEADS, JOURNALS], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(STORE).delete(projectId)
    for (const snapshot of snapshots) transaction.objectStore(SNAPSHOTS).delete(snapshot.key)
    transaction.objectStore(HEADS).delete(projectId)
    for (const journal of journals) transaction.objectStore(JOURNALS).delete(journal.id)
    await committed
  } finally {
    database.close()
  }
}
