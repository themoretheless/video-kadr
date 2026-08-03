import 'fake-indexeddb/auto'

import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { ProjectDto } from './api'
import {
  allProjects,
  compareAndSwapProject,
  discardProjectRecovery,
  inspectProjectRecovery,
  prepareProjectDraft,
  projectByVideo,
  recoverProject,
} from './browser-project-store'
import { applyTimelineCommand } from './domain/timeline'
import { ensureCreatorTrackLayout, migrateProjectDocument } from './project-schema'

function clearDatabase(): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase('video-kadr')
    request.onsuccess = () => resolve()
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error('IndexedDB deletion blocked'))
  })
}

function project(id: string, videoId: string, revision: number, marker: string): ProjectDto {
  return {
    id,
    name: marker,
    videoId,
    video: { id: videoId, filename: 'clip.mp4', url: 'blob:test', duration: 1, width: 1, height: 1 },
    edit: { filter: marker },
    revision,
    createdAt: 1,
    updatedAt: revision,
  }
}

async function openTestDatabase(): Promise<IDBDatabase> {
  return await new Promise((resolve, reject) => {
    const request = indexedDB.open('video-kadr', 2)
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}

async function journalRows(projectId: string): Promise<Array<Record<string, unknown>>> {
  const database = await openTestDatabase()
  try {
    const request = database.transaction('project-journals').objectStore('project-journals').index('projectId').getAll(projectId)
    return await new Promise((resolve, reject) => {
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
  } finally { database.close() }
}

describe('browser project persistence', () => {
  beforeEach(clearDatabase)

  it('restores a committed project after memory-independent reload', async () => {
    await compareAndSwapProject(
      'media-1',
      'project-1',
      0,
      () => project('project-1', 'media-1', 1, 'saved'),
      () => new Error('conflict'),
    )
    const restored = await allProjects()
    expect(restored).toHaveLength(1)
    expect(restored[0]).toMatchObject({ id: 'project-1', revision: 1, name: 'saved' })
  })

  it('round-trips a canonical twenty-one-media timeline through IndexedDB', async () => {
    let document = ensureCreatorTrackLayout(migrateProjectDocument({
      videoId: 'primary',
      video: { id: 'primary', filename: 'primary.mp4', duration: 1, width: 1920, height: 1080 },
      edit: {},
    }))
    const track = document.sequences[0]!.tracks.find((item) => item.kind === 'video')!
    for (let index = 0; index < 20; index++) {
      document = applyTimelineCommand(document, {
        kind: 'insert_media_clip',
        sequenceId: document.activeSequenceId,
        trackId: track.id,
        index: index + 1,
        media: {
          id: `media-${index}`,
          kind: 'video',
          metadata: { duration: 1, fps: [23.976, 25, 29.97, 59.94][index % 4] },
        },
        clip: {
          id: `clip-${index}`,
          mediaId: `media-${index}`,
          timelineStartTick: (index + 1) * 1_000_000,
          durationTicks: 1_000_000,
          sourceInTick: 0,
          sourceOutTick: 1_000_000,
          effects: [],
        },
      })
    }
    const dto: ProjectDto = {
      ...project('project-many', 'primary', 1, 'many'),
      document,
    }
    await compareAndSwapProject(
      'primary', 'project-many', 0, () => dto, () => new Error('conflict'),
    )

    const restored = await projectByVideo('primary')
    expect(restored?.document).toEqual(document)
    expect(restored?.document?.media).toHaveLength(21)
    expect(restored?.document?.sequences[0]!.tracks.flatMap((item) => item.clips))
      .toHaveLength(21)
  })

  it('atomically rejects one of two stale writers without overwriting the winner', async () => {
    await compareAndSwapProject(
      'media-1',
      'project-1',
      0,
      () => project('project-1', 'media-1', 1, 'initial'),
      () => new Error('conflict'),
    )
    const write = (marker: string) =>
      compareAndSwapProject(
        'media-1',
        'project-1',
        1,
        () => project('project-1', 'media-1', 2, marker),
        () => new Error('conflict'),
      )
    const results = await Promise.allSettled([write('tab-a'), write('tab-b')])
    expect(results.filter((result) => result.status === 'fulfilled')).toHaveLength(1)
    expect(results.filter((result) => result.status === 'rejected')).toHaveLength(1)
    const [stored] = await allProjects()
    expect(stored?.revision).toBe(2)
    expect(['tab-a', 'tab-b']).toContain(stored?.name)
  })

  it('keeps two projects that reference the same media source', async () => {
    for (const id of ['project-a', 'project-b']) {
      await compareAndSwapProject(
        'shared-media',
        id,
        0,
        () => project(id, 'shared-media', 1, id),
        () => new Error('conflict'),
      )
    }
    expect((await allProjects()).map((item) => item.id).sort()).toEqual([
      'project-a',
      'project-b',
    ])
  })

  it('rejects update of a missing project', async () => {
    await expect(
      compareAndSwapProject(
        'media-1',
        'missing',
        4,
        () => project('missing', 'media-1', 5, 'bad'),
        () => new Error('conflict'),
      ),
    ).rejects.toThrow('conflict')
    expect(await allProjects()).toEqual([])
  })

  it('recovers a complete prepared draft after the head transaction aborts', async () => {
    await compareAndSwapProject(
      'media-1', 'project-1', 0,
      () => project('project-1', 'media-1', 1, 'committed'),
      () => new Error('conflict'),
    )
    const nativeAdd = IDBObjectStore.prototype.add
    const add = vi.spyOn(IDBObjectStore.prototype, 'add').mockImplementation(function (
      this: IDBObjectStore, value: unknown, key?: IDBValidKey,
    ) {
      if (this.name === 'project-snapshots' && (value as { revision?: number }).revision === 2) {
        throw new DOMException('fault inside head transaction', 'AbortError')
      }
      return key === undefined ? nativeAdd.call(this, value) : nativeAdd.call(this, value, key)
    })
    await expect(compareAndSwapProject(
      'media-1', 'project-1', 1,
      () => project('project-1', 'media-1', 2, 'prepared draft'),
      () => new Error('conflict'),
    )).rejects.toBeDefined()
    add.mockRestore()

    expect(await projectByVideo('media-1')).toMatchObject({ revision: 1, name: 'committed' })
    const recovery = await inspectProjectRecovery('project-1')
    expect(recovery).toMatchObject({ journal: true, candidateRevision: 2, candidate: { name: 'prepared draft' } })
    const restored = await recoverProject('project-1', 1, 2, recovery?.journalId)
    expect(restored).toMatchObject({ revision: 2, name: 'prepared draft' })
    expect(await inspectProjectRecovery('project-1')).toBeNull()
  })

  it('persists a pre-debounce draft and consumes it after the matching CAS commit', async () => {
    await compareAndSwapProject(
      'media-1', 'project-1', 0,
      () => project('project-1', 'media-1', 1, 'committed'),
      () => new Error('conflict'),
    )
    const draft = project('project-1', 'media-1', 2, 'dirty before debounce')
    const watermark = await prepareProjectDraft(draft, 1)
    expect(await inspectProjectRecovery('project-1')).toMatchObject({
      reason: 'draft', candidateRevision: 2, candidate: { name: 'dirty before debounce' },
    })
    await compareAndSwapProject(
      'media-1', 'project-1', 1, () => draft, () => new Error('conflict'), watermark,
    )
    expect(await inspectProjectRecovery('project-1')).toBeNull()
  })

  it('recovers the first local draft when no committed head exists yet', async () => {
    const draft = project('new-project', 'new-media', 1, 'first dirty edit')
    await prepareProjectDraft(draft, 0)
    const recovery = await inspectProjectRecovery('new-project')
    expect(recovery).toMatchObject({ corruptRevision: 0, candidateRevision: 1, reason: 'draft' })
    await expect(recoverProject('new-project', 0, 1, recovery?.journalId)).resolves.toMatchObject({
      revision: 1, name: 'first dirty edit',
    })
    expect(await projectByVideo('new-media')).toMatchObject({ revision: 1, name: 'first dirty edit' })
  })

  it('publishes either a whole prepared draft or no draft when its transaction aborts', async () => {
    const nativePut = IDBObjectStore.prototype.put
    const put = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (
      this: IDBObjectStore, value: unknown, key?: IDBValidKey,
    ) {
      if (this.name === 'project-journals') throw new DOMException('draft write fault', 'AbortError')
      return key === undefined ? nativePut.call(this, value) : nativePut.call(this, value, key)
    })
    await expect(prepareProjectDraft(
      project('atomic-draft', 'atomic-media', 1, 'never partial'), 0,
    )).rejects.toBeDefined()
    put.mockRestore()
    expect(await inspectProjectRecovery('atomic-draft')).toBeNull()
    expect(await projectByVideo('atomic-media')).toBeNull()
  })

  it('discards a prepared draft without changing the committed head', async () => {
    await compareAndSwapProject(
      'media-1', 'project-1', 0,
      () => project('project-1', 'media-1', 1, 'committed'),
      () => new Error('conflict'),
    )
    const nativeAdd = IDBObjectStore.prototype.add
    const add = vi.spyOn(IDBObjectStore.prototype, 'add').mockImplementation(function (
      this: IDBObjectStore, value: unknown, key?: IDBValidKey,
    ) {
      if (this.name === 'project-snapshots' && (value as { revision?: number }).revision === 2) throw new Error('crash')
      return key === undefined ? nativeAdd.call(this, value) : nativeAdd.call(this, value, key)
    })
    await expect(compareAndSwapProject(
      'media-1', 'project-1', 1,
      () => project('project-1', 'media-1', 2, 'discard me'),
      () => new Error('conflict'),
    )).rejects.toBeDefined()
    add.mockRestore()
    const recovery = await inspectProjectRecovery('project-1')
    await discardProjectRecovery('project-1', recovery!.journalId!)
    expect(await inspectProjectRecovery('project-1')).toBeNull()
    expect(await projectByVideo('media-1')).toMatchObject({ revision: 1, name: 'committed' })
  })

  it('rolls a corrupt head forward to the nearest valid snapshot', async () => {
    await compareAndSwapProject('media-1', 'project-1', 0, () => project('project-1', 'media-1', 1, 'safe'), () => new Error('conflict'))
    await compareAndSwapProject('media-1', 'project-1', 1, () => project('project-1', 'media-1', 2, 'latest'), () => new Error('conflict'))
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr', 2)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const read = database.transaction('project-snapshots').objectStore('project-snapshots').get('project-1:2')
    const corrupt = await new Promise<Record<string, unknown>>((resolve, reject) => {
      read.onsuccess = () => resolve(read.result)
      read.onerror = () => reject(read.error)
    })
    const transaction = database.transaction('project-snapshots', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('project-snapshots').put({ ...corrupt, checksum: '0'.repeat(64) })
    await committed
    database.close()

    const recovery = await inspectProjectRecovery('project-1')
    expect(recovery).toMatchObject({ corruptRevision: 2, candidateRevision: 1, candidate: { name: 'safe' } })
    const restored = await recoverProject('project-1', 2, 1)
    expect(restored).toMatchObject({ revision: 3, name: 'safe' })
  })

  it('detects a corrupt mutable head even when its immutable snapshot is valid', async () => {
    await compareAndSwapProject('media-1', 'project-1', 0, () => project('project-1', 'media-1', 1, 'safe'), () => new Error('conflict'))
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr', 2)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const transaction = database.transaction('projects', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('projects').put({ ...project('project-1', 'media-1', 1, 'tampered'), document: { schemaVersion: 999 } })
    await committed
    database.close()
    const recovery = await inspectProjectRecovery('project-1')
    expect(recovery).toMatchObject({ reason: 'corrupt', corruptRevision: 1, candidateRevision: 1, candidate: { name: 'safe' } })
    const restored = await recoverProject('project-1', 1, 1)
    expect(restored).toMatchObject({ revision: 2, name: 'safe' })
  })

  it('does not let an older save acknowledgement delete a newer same-revision draft', async () => {
    await compareAndSwapProject('media-1', 'project-1', 0, () => project('project-1', 'media-1', 1, 'head'), () => new Error('conflict'))
    const acknowledged = project('project-1', 'media-1', 2, 'older draft')
    await prepareProjectDraft(acknowledged, 1)
    await prepareProjectDraft(project('project-1', 'media-1', 2, 'newer draft'), 1)

    await compareAndSwapProject('media-1', 'project-1', 1, () => acknowledged, () => new Error('conflict'))

    expect(await inspectProjectRecovery('project-1')).toMatchObject({
      reason: 'draft', candidate: { name: 'newer draft' }, candidateRevision: 2,
    })
  })

  it('discard and recover preserve newer and other-writer journals', async () => {
    await compareAndSwapProject('media-1', 'project-1', 0, () => project('project-1', 'media-1', 1, 'head'), () => new Error('conflict'))
    await prepareProjectDraft(project('project-1', 'media-1', 2, 'selected'), 1)
    const selected = await inspectProjectRecovery('project-1')
    await prepareProjectDraft(project('project-1', 'media-1', 2, 'newer'), 1)
    const rows = await journalRows('project-1')
    const source = rows.find((row) => row.id === selected!.journalId)!
    const database = await openTestDatabase()
    const transaction = database.transaction('project-journals', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('project-journals').put({ ...source, id: 'other-writer', writerId: 'other-tab', sequence: Number(source.sequence) - 1 })
    await committed
    database.close()

    await discardProjectRecovery('project-1', selected!.journalId!)
    expect((await journalRows('project-1')).map((row) => row.id)).toContain('other-writer')
    expect(await inspectProjectRecovery('project-1')).toMatchObject({ candidate: { name: 'newer' } })

    const newer = await inspectProjectRecovery('project-1')
    await recoverProject('project-1', 1, 2, newer!.journalId)
    const remaining = await journalRows('project-1')
    expect(remaining.map((row) => row.id)).toContain('other-writer')
    expect(remaining.some((row) => row.id === newer!.journalId)).toBe(false)
  })

  it('recovering an older journal preserves a newer same-writer journal and another writer', async () => {
    await compareAndSwapProject('media-r', 'project-r', 0, () => project('project-r', 'media-r', 1, 'head'), () => new Error('conflict'))
    await prepareProjectDraft(project('project-r', 'media-r', 2, 'selected old'), 1)
    const selected = await inspectProjectRecovery('project-r')
    await prepareProjectDraft(project('project-r', 'media-r', 2, 'newer survives'), 1)
    const source = (await journalRows('project-r')).find((row) => row.id === selected!.journalId)!
    const database = await openTestDatabase()
    const transaction = database.transaction('project-journals', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('project-journals').put({ ...source, id: 'other-writer-r', writerId: 'other-tab' })
    await committed
    database.close()

    await recoverProject('project-r', 1, 2, selected!.journalId)
    const remaining = await journalRows('project-r')
    expect(remaining.map((row) => row.id)).toContain('other-writer-r')
    expect(await inspectProjectRecovery('project-r')).toMatchObject({ candidate: { name: 'newer survives' } })
  })

  it('chooses the newest draft deterministically when timestamps are equal', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(42)
    await prepareProjectDraft(project('project-1', 'media-1', 1, 'first'), 0)
    await prepareProjectDraft(project('project-1', 'media-1', 1, 'second'), 0)
    expect(await inspectProjectRecovery('project-1')).toMatchObject({ candidate: { name: 'second' } })
    vi.restoreAllMocks()
  })

  it('isolates a checksum-divergent row and returns its valid immutable snapshot from allProjects', async () => {
    await compareAndSwapProject('media-1', 'project-1', 0, () => project('project-1', 'media-1', 1, 'safe'), () => new Error('conflict'))
    await compareAndSwapProject('media-2', 'project-2', 0, () => project('project-2', 'media-2', 1, 'other safe'), () => new Error('conflict'))
    const database = await openTestDatabase()
    const transaction = database.transaction('projects', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('projects').put(project('project-1', 'media-1', 1, 'valid schema but corrupt bytes'))
    await committed
    database.close()

    expect((await allProjects()).map((item) => item.name).sort()).toEqual(['other safe', 'safe'])
  })

  it('never trusts a mutable head when its bound snapshot is missing', async () => {
    await compareAndSwapProject(
      'media-1', 'project-1', 0,
      () => project('project-1', 'media-1', 1, 'safe revision one'),
      () => new Error('conflict'),
    )
    await compareAndSwapProject(
      'media-1', 'project-1', 1,
      () => project('project-1', 'media-1', 2, 'safe revision two'),
      () => new Error('conflict'),
    )
    const database = await openTestDatabase()
    const transaction = database.transaction(['projects', 'project-snapshots'], 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('project-snapshots').delete('project-1:2')
    transaction.objectStore('projects').put(
      project('project-1', 'media-1', 2, 'schema-valid tampered mutable head'),
    )
    await committed
    database.close()

    expect(await allProjects()).toEqual([
      expect.objectContaining({ id: 'project-1', revision: 1, name: 'safe revision one' }),
    ])
  })
})
