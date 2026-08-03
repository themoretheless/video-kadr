import { createApp, nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'

import { migrateProjectDocument, ensureCreatorTrackLayout } from '../project-schema'
import { state, timelineState } from '../store'
import MediaLibrary from './MediaLibrary.vue'

let host: HTMLElement | null = null

afterEach(() => {
  host?.remove()
  host = null
  state.library = []
  state.projects = []
  timelineState.document = null
  timelineState.selectedClipId = null
})

function mountLibrary(): HTMLElement {
  host = document.createElement('div')
  document.body.append(host)
  createApp(MediaLibrary).mount(host)
  return host
}

describe('MediaLibrary project actions', () => {
  it('keeps Open separate from Add and inserts into the active project', async () => {
    timelineState.document = ensureCreatorTrackLayout(migrateProjectDocument({
      videoId: 'primary',
      video: { id: 'primary', filename: 'primary.mp4', duration: 5, width: 1280, height: 720 },
      edit: {},
    }))
    timelineState.selectedClipId = 'clip-main'
    state.library = [{
      id: 'secondary',
      kind: 'source',
      filename: 'secondary.mp4',
      url: '/files/sources/secondary.mp4',
      duration: 3,
      width: 1920,
      height: 1080,
      fps: 25,
      createdAt: 2,
    }]
    const element = mountLibrary()
    expect([...element.querySelectorAll('button')].map((button) => button.textContent?.trim()))
      .toEqual(['Открыть как проект', 'Добавить', '✕'])

    element.querySelector<HTMLButtonElement>('[aria-label^="Добавить"]')!.click()
    await nextTick()
    expect(timelineState.document.primaryMediaId).toBe('primary')
    expect(timelineState.document.media.map((media) => media.id)).toEqual(['primary', 'secondary'])
    expect(timelineState.document.sequences[0]!.tracks[0]!.clips).toHaveLength(2)
  })

  it('does not render Add before a project is open', () => {
    state.library = [{
      id: 'source', kind: 'source', filename: 'source.mp4', url: '/source',
      duration: 1, width: 640, height: 360, createdAt: 1,
    }]
    const element = mountLibrary()
    expect(element.querySelector('[aria-label^="Добавить"]')).toBeNull()
  })

  it('shows an actionable offline state without dropping the library item', () => {
    timelineState.document = ensureCreatorTrackLayout(migrateProjectDocument({
      videoId: 'primary',
      video: { id: 'primary', filename: 'primary.mp4', duration: 5, width: 1280, height: 720 },
      edit: {},
    }))
    state.library = [{
      id: 'offline', kind: 'source', filename: 'offline.mp4', url: '',
      duration: 1, width: 640, height: 360, mediaKind: 'video',
      availability: 'offline', fingerprint: 'abc', createdAt: 1,
    }]
    const element = mountLibrary()
    expect(element.querySelector('[role="status"]')?.textContent).toContain('недоступен')
    expect(element.textContent).toContain('Найти файл')
    expect(element.querySelector<HTMLButtonElement>('[aria-label^="Добавить"]')?.disabled).toBe(true)
    expect(state.library).toHaveLength(1)
    expect(timelineState.document.media.map((media) => media.id)).toEqual(['primary'])
  })

  it('shows project-level missing media progress and a multi-file picker', () => {
    timelineState.document = ensureCreatorTrackLayout(migrateProjectDocument({
      videoId: 'offline-a',
      video: { id: 'offline-a', filename: 'a.mp4', duration: 5, width: 1280, height: 720 },
      edit: {},
    }))
    timelineState.document.media.push({
      id: 'offline-b', assetRef: 'offline-b', name: 'b.mp4', kind: 'video',
      metadata: { id: 'offline-b', filename: 'b.mp4', duration: 2, width: 640, height: 360 },
    })
    state.library = [
      { id: 'offline-a', kind: 'source', filename: 'a.mp4', url: '', availability: 'offline', createdAt: 2 },
      { id: 'offline-b-row', assetId: 'offline-b', kind: 'source', filename: 'b.mp4', url: '', availability: 'offline', createdAt: 1 },
    ]
    const element = mountLibrary()
    expect(element.querySelector('.relink-summary')?.textContent).toContain('Недоступно исходников: 2')
    expect(element.querySelector<HTMLInputElement>('.relink-summary input[type=file]')?.multiple).toBe(true)
  })

  it('does not classify server and session sources as missing', () => {
    timelineState.document = ensureCreatorTrackLayout(migrateProjectDocument({
      videoId: 'server-source',
      video: { id: 'server-source', filename: 'server.mp4', duration: 5, width: 1280, height: 720 },
      edit: {},
    }))
    timelineState.document.media.push({ id: 'session-source', assetRef: 'session-source', kind: 'video', metadata: {} })
    state.library = [
      { id: 'server-source', kind: 'source', filename: 'server.mp4', url: '/server.mp4', createdAt: 2 },
      { id: 'session-source', kind: 'source', filename: 'session.mp4', url: 'blob:session', availability: 'session', createdAt: 1 },
    ]
    expect(mountLibrary().querySelector('.relink-summary')).toBeNull()
  })

  it('renders distinct project choices when one source has multiple projects', () => {
    const video = { id: 'shared', url: '/shared', filename: 'shared.mp4', duration: 1, width: 1, height: 1 }
    state.projects = [
      { id: 'project-a', name: 'Версия A', videoId: 'shared', video, edit: {}, createdAt: 1, updatedAt: 2 },
      { id: 'project-b', name: 'Версия B', videoId: 'shared', video, edit: {}, createdAt: 1, updatedAt: 3 },
    ]
    state.library = [{ id: 'shared', kind: 'source', filename: 'shared.mp4', url: '/shared', createdAt: 1 }]
    const labels = [...mountLibrary().querySelectorAll<HTMLButtonElement>('.project-choices button')]
      .map((button) => button.getAttribute('aria-label'))
    expect(labels).toEqual(['Открыть проект Версия A', 'Открыть проект Версия B'])
  })

  it('treats a globally ready secondary with another project fingerprint as missing', () => {
    timelineState.document = ensureCreatorTrackLayout(migrateProjectDocument({
      videoId: 'primary',
      video: { id: 'primary', filename: 'primary.mp4', duration: 5, width: 1280, height: 720 },
      edit: {},
    }))
    timelineState.document.media.push({
      id: 'secondary', assetRef: 'shared-secondary', kind: 'video',
      contentFingerprint: 'project-b-fingerprint', metadata: { filename: 'secondary.mp4' },
    })
    state.library = [{
      id: 'shared-secondary', assetId: 'shared-secondary', kind: 'source', filename: 'secondary.mp4',
      url: 'blob:project-a', availability: 'ready', fingerprint: 'project-a-fingerprint', createdAt: 1,
    }]
    expect(mountLibrary().querySelector('.relink-summary')?.textContent).toContain('Недоступно исходников: 1')
  })
})
