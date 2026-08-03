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
})
