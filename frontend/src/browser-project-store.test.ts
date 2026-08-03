import 'fake-indexeddb/auto'

import { beforeEach, describe, expect, it } from 'vitest'

import type { ProjectDto } from './api'
import { allProjects, compareAndSwapProject, projectByVideo } from './browser-project-store'
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
})
