import type { ProjectDto } from './api'

const DATABASE = 'video-kadr'
const VERSION = 1
const STORE = 'projects'

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
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('Не удалось открыть IndexedDB'))
  })
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
  const database = await openDatabase()
  try {
    const transaction = database.transaction(STORE, 'readwrite')
    const committed = transactionDone(transaction)
    await requestResult(transaction.objectStore(STORE).put(structuredClone(project)))
    await committed
  } finally {
    database.close()
  }
}

/** One read/write transaction prevents two tabs from accepting the same revision. */
export async function compareAndSwapProject(
  videoId: string,
  projectId: string | undefined,
  expectedRevision: number | undefined,
  build: (previous: ProjectDto | undefined) => ProjectDto,
  conflict: () => Error,
): Promise<ProjectDto> {
  const database = await openDatabase()
  try {
    return await new Promise<ProjectDto>((resolve, reject) => {
      const transaction = database.transaction(STORE, 'readwrite')
      const store = transaction.objectStore(STORE)
      const read = (projectId ? store.get(projectId) : store.index('videoId').get(videoId)) as IDBRequest<ProjectDto | undefined>
      let next: ProjectDto | undefined
      read.onerror = () => reject(read.error ?? new Error('Ошибка IndexedDB'))
      read.onsuccess = () => {
        const previous = read.result
        if (
          expectedRevision !== undefined &&
          ((!previous && expectedRevision !== 0) ||
            (previous && (previous.revision ?? 0) !== expectedRevision))
        ) {
          transaction.abort()
          reject(conflict())
          return
        }
        try {
          next = build(previous)
          store.put(structuredClone(next))
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
    })
  } finally {
    database.close()
  }
}

export async function projectByVideo(videoId: string): Promise<ProjectDto | null> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(STORE, 'readonly')
    const result = await requestResult<ProjectDto | undefined>(
      transaction.objectStore(STORE).index('videoId').get(videoId),
    )
    return result ?? null
  } finally {
    database.close()
  }
}

export async function allProjects(): Promise<ProjectDto[]> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(STORE, 'readonly')
    const projects = await requestResult<ProjectDto[]>(transaction.objectStore(STORE).getAll())
    return projects.sort((left, right) => right.updatedAt - left.updatedAt)
  } finally {
    database.close()
  }
}

export async function removeProject(projectId: string): Promise<void> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(STORE, 'readwrite')
    const committed = transactionDone(transaction)
    await requestResult(transaction.objectStore(STORE).delete(projectId))
    await committed
  } finally {
    database.close()
  }
}
