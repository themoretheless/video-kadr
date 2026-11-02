// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { listFilmcraftExportPresets, editFilmcraftExportPresets, type FilmcraftLibraryPreset } from '$lib/api'
import Library from './FilmcraftExportPresetLibrary.svelte'
vi.mock('$lib/api', () => ({ listFilmcraftExportPresets: vi.fn(), editFilmcraftExportPresets: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
const builtin: FilmcraftLibraryPreset = { name: 'YouTube 1080p Full HD', category: 'Web', description: 'delivery', settings: {}, builtin: true, favorite: false }
const user: FilmcraftLibraryPreset = { ...builtin, name: 'Review 720p', category: 'User', builtin: false }
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
async function input(label: string, value: string) { const input = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.innerHTML = ''; vi.resetAllMocks() })

it('saves a derived preset with explicit size and bitrate then applies the stored name', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValueOnce({ presets: [builtin], default: builtin.name }).mockResolvedValue({ presets: [builtin, user], default: builtin.name })
  vi.mocked(editFilmcraftExportPresets).mockResolvedValue({ name: user.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } }); await tick()
  expect(listFilmcraftExportPresets).not.toHaveBeenCalled()
  button('Библиотека пресетов').click(); await vi.waitFor(() => expect(button('Удалить пресет').disabled).toBe(true))
  await input('Название нового пресета Filmcraft', user.name)
  await input('Ширина нового пресета Filmcraft', '1280'); await input('Высота нового пресета Filmcraft', '720'); await input('Битрейт нового пресета Filmcraft', '5000')
  button('Сохранить копию пресета').click()
  await vi.waitFor(() => expect(editFilmcraftExportPresets).toHaveBeenCalledWith('alice', { action: 'save', name: user.name, from: builtin.name, settings: { frameSize: [1280, 720], bitrateKbps: 5000, adaptiveBitrate: null }, overwrite: false }, expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Удалить пресет').disabled).toBe(false))
  button('Применить пресет').click(); expect(apply).toHaveBeenCalledWith(user.name)
})

it('aborts loading on unmount and suppresses late owner data', async () => {
  let resolve!: (value: { presets: FilmcraftLibraryPreset[]; default: string }) => void
  vi.mocked(listFilmcraftExportPresets).mockReturnValue(new Promise(r => { resolve = r }))
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } }); await tick()
  button('Библиотека пресетов').click(); await tick()
  const signal = vi.mocked(listFilmcraftExportPresets).mock.calls[0][2]!
  await unmount(component); component = undefined
  expect(signal.aborted).toBe(true)
  resolve({ presets: [user], default: user.name }); await tick()
  expect(document.body.textContent).not.toContain(user.name)
})

it('saves match-source scale by clearing the inherited fixed frame size', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [builtin], default: builtin.name })
  vi.mocked(editFilmcraftExportPresets).mockResolvedValue({ name: 'Half source' })
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } }); await tick()
  button('Библиотека пресетов').click(); await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  await input('Название нового пресета Filmcraft', 'Half source')
  document.querySelector<HTMLInputElement>('[aria-label="Размер секвенции нового пресета Filmcraft"]')!.click(); await tick()
  await input('Масштаб нового пресета Filmcraft', '0.5')
  expect(document.querySelector<HTMLInputElement>('[aria-label="Ширина нового пресета Filmcraft"]')!.disabled).toBe(true)
  button('Сохранить копию пресета').click()
  await vi.waitFor(() => expect(editFilmcraftExportPresets).toHaveBeenCalledWith('alice', { action: 'save', name: 'Half source', from: builtin.name, settings: { frameSize: null, scale: 0.5 }, overwrite: false }, expect.any(AbortSignal)))
})

it('stores an explicit caption burn choice in the saved source settings', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [builtin], default: builtin.name })
  vi.mocked(editFilmcraftExportPresets).mockResolvedValue({ name: 'Caption review' })
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } }); await tick()
  button('Библиотека пресетов').click(); await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  await input('Название нового пресета Filmcraft', 'Caption review')
  const select=document.querySelector<HTMLSelectElement>('[aria-label="Субтитры нового пресета Filmcraft"]')!;select.value='burn';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
  button('Сохранить копию пресета').click()
  await vi.waitFor(() => expect(editFilmcraftExportPresets).toHaveBeenCalledWith('alice', { action: 'save', name: 'Caption review', from: builtin.name, settings: { burnCaptions: true }, overwrite: false }, expect.any(AbortSignal)))
})

it('stores an H.264 level using the source integer representation', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [{ ...builtin, settings: { format: 'h264' } }], default: builtin.name })
  vi.mocked(editFilmcraftExportPresets).mockResolvedValue({ name: 'Level review' })
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } }); await tick()
  button('Библиотека пресетов').click(); await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  await input('Название нового пресета Filmcraft', 'Level review')
  const select=document.querySelector<HTMLSelectElement>('[aria-label="Уровень H.264 нового пресета Filmcraft"]')!;select.value='41';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
  button('Сохранить копию пресета').click()
  await vi.waitFor(() => expect(editFilmcraftExportPresets).toHaveBeenCalledWith('alice', { action: 'save', name: 'Level review', from: builtin.name, settings: { h264Level: 41 }, overwrite: false }, expect.any(AbortSignal)))
})

it('saves a separate binary caption format without enabling caption burn', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [builtin], default: builtin.name })
  vi.mocked(editFilmcraftExportPresets).mockResolvedValue({ name: 'STL delivery' })
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } }); await tick()
  button('Библиотека пресетов').click(); await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  await input('Название нового пресета Filmcraft', 'STL delivery')
  const select=document.querySelector<HTMLSelectElement>('[aria-label="Файл субтитров нового пресета Filmcraft"]')!;select.value='stl';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
  button('Сохранить копию пресета').click()
  await vi.waitFor(() => expect(editFilmcraftExportPresets).toHaveBeenCalledWith('alice', { action: 'save', name: 'STL delivery', from: builtin.name, settings: { captionSidecar: 'stl' }, overwrite: false }, expect.any(AbortSignal)))
})

it('rejects a malformed import without sending it to the backend', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [builtin], default: builtin.name })
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } }); await tick()
  button('Библиотека пресетов').click(); await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  const input = document.querySelector<HTMLInputElement>('[aria-label="Импорт пресетов Filmcraft"]')!
  Object.defineProperty(input, 'files', { value: [{ size: 4, text: async () => 'oops' }] })
  input.dispatchEvent(new Event('change', { bubbles: true }))
  await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull())
  expect(editFilmcraftExportPresets).not.toHaveBeenCalled()
})

it('downloads a portable preset file with an authenticated export request', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [builtin], default: builtin.name })
  const file = { format: 'filmcraft.export-presets', version: 1, presets: [builtin], favorites: [] }
  vi.mocked(editFilmcraftExportPresets).mockResolvedValue(file)
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:preset')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe('filmcraft-export-presets.json') })
  try {
    component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } }); await tick()
    button('Библиотека пресетов').click(); await vi.waitFor(() => expect(button('Скачать пресет')).toBeDefined())
    button('Скачать пресет').click()
    await vi.waitFor(() => expect(click).toHaveBeenCalledOnce())
    expect(editFilmcraftExportPresets).toHaveBeenCalledWith('alice', { action: 'export', names: [builtin.name] }, expect.any(AbortSignal))
    expect(create).toHaveBeenCalledWith(expect.any(Blob)); expect(revoke).toHaveBeenCalledWith('blob:preset')
  } finally { create.mockRestore(); revoke.mockRestore(); click.mockRestore() }
})
