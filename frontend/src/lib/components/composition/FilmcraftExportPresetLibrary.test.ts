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

it('applies PCM options to an audio-only preset and keeps its codec fixed', async () => {
  const wav = { ...builtin, name: 'Wave', settings: { format: 'wav', audio: { codec: 'auto', sample_rate: 48000 } } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [wav], default: wav.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  expect(document.querySelector('[aria-label="Аудиокодек пресета Filmcraft"]')).toBeNull()
  expect(document.querySelector('[aria-label="Битрейт AAC пресета Filmcraft"]')).toBeNull()
  for (const [label, value] of [
    ['Частота аудио пресета Filmcraft', 'source'],
    ['Каналы пресета Filmcraft', '6'],
    ['Разрядность PCM пресета Filmcraft', '24'],
  ]) {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value
    control.dispatchEvent(new Event('change', { bubbles: true }))
  }
  await tick()
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenCalledWith('Wave', { audio: { sample_rate: null, channels: 6, bits: 24 } })
  expect(editFilmcraftExportPresets).not.toHaveBeenCalled()
})

it('applies MOV, H.264 profile, rate control and keyframe settings together', async () => {
  const h264 = { ...builtin, settings: { format: 'h264', multiplexer: 'mp4' } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [h264], default: h264.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  expect(document.querySelector('[aria-label="Аудиокодек пресета Filmcraft"]')).toBeNull()
  for (const [label, value] of [
    ['Контейнер пресета Filmcraft', 'mov'],
    ['Профиль H.264 пресета Filmcraft', 'main'],
    ['Режим битрейта пресета Filmcraft', 'vbr2Pass'],
  ]) {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value
    control.dispatchEvent(new Event('change', { bubbles: true }))
  }
  await tick()
  expect(document.querySelector('[aria-label="Аудиокодек пресета Filmcraft"]')).not.toBeNull()
  await input('Максимальный битрейт пресета Filmcraft', '800')
  await input('Адаптивный битрейт пресета Filmcraft', '0.5')
  await input('Интервал ключевых кадров пресета Filmcraft', '4')
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenCalledWith(h264.name, {
    multiplexer: 'mov', h264Profile: 'main', bitrateMode: 'vbr2Pass',
    maxBitrateKbps: 800, adaptiveBitrate: 0.5, keyframeDistance: 4,
  })
})

it('applies ProRes profile, pixel aspect and render quality as one draft', async () => {
  const preset = { ...builtin, settings: { format: 'prores' } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [preset], default: preset.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  for (const [label, value] of [
    ['Пропорции пикселя пресета Filmcraft', 'custom'],
    ['Качество рендера пресета Filmcraft', 'maximum'],
    ['Профиль видеокодека пресета Filmcraft', 'lt'],
  ]) {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value
    control.dispatchEvent(new Event('change', { bubbles: true }))
  }
  await tick()
  await input('Числитель пропорций пикселя пресета Filmcraft', '4')
  await input('Знаменатель пропорций пикселя пресета Filmcraft', '0')
  expect(button('Применить правки к экспорту').disabled).toBe(true)
  await input('Знаменатель пропорций пикселя пресета Filmcraft', '3')
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenCalledWith(preset.name, { proresProfile: 'lt', pixelAspect: [4, 3], maxRenderQuality: true })
})

it('inherits untouched metadata and allows clearing a saved value for one export', async () => {
  const preset = { ...builtin, settings: { format: 'h264', metadata: { title: 'Original', creator: 'Editor', comment: 'Old' } } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [preset], default: preset.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  const creator = document.querySelector<HTMLInputElement>('[aria-label="Автор в пресете Filmcraft"]')!
  expect(creator.value).toBe('Editor')
  await input('Название в пресете Filmcraft', 'Новый монтаж')
  await input('Комментарий в пресете Filmcraft', '')
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenCalledWith(preset.name, { metadata: { title: 'Новый монтаж', comment: '' } })
  expect(editFilmcraftExportPresets).not.toHaveBeenCalled()
  button('Восстановить метаданные пресета').click()
  await tick()
  expect(document.querySelector<HTMLInputElement>('[aria-label="Название в пресете Filmcraft"]')?.value).toBe('Original')
  expect(button('Применить правки к экспорту').disabled).toBe(true)
})

it('applies portable limiter and loudness effects with inherited starting values', async () => {
  const preset = { ...builtin, settings: { format: 'h264', effects: { video_limiter: { min_percent: 8, max_percent: 82 } } } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [preset], default: preset.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  for (const label of ['Ограничитель яркости пресета Filmcraft', 'Нормализация громкости пресета Filmcraft']) {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = 'on'
    control.dispatchEvent(new Event('change', { bubbles: true }))
  }
  await tick()
  await input('Верхний порог яркости пресета Filmcraft', '101')
  expect(button('Применить правки к экспорту').disabled).toBe(true)
  await input('Верхний порог яркости пресета Filmcraft', '75')
  await input('Целевая громкость пресета Filmcraft', '-18')
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenCalledWith(preset.name, { effects: {
    video_limiter: { enabled: true, min_percent: 8, max_percent: 75 },
    loudness: { enabled: true, target_lufs: -18, true_peak_dbtp: -2 },
  } })
})

it('overrides caption text and disables timecode while retaining inherited placement', async () => {
  const preset = { ...builtin, settings: { format: 'png', effects: { name_overlay: { placement: 'topLeft' } } } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [preset], default: preset.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  for (const [label, value] of [['Подпись', 'on'], ['Таймкод', 'off']]) {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label} в пресете Filmcraft"]`)!
    control.value = value
    control.dispatchEvent(new Event('change', { bubbles: true }))
  }
  await tick()
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Подпись: положение в пресете Filmcraft"]')?.value).toBe('topLeft')
  await input('Подпись: текст пресета Filmcraft', 'Review')
  await input('Подпись: X в пресете Filmcraft', '12')
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenCalledWith(preset.name, { effects: {
    name_overlay: { enabled: true, text: 'Review', offset: [12, 0] },
    timecode_overlay: { enabled: false },
  } })
})

it('requires a library image and applies image overlay geometry to a preset', async () => {
  const preset = { ...builtin, settings: { format: 'png' } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [preset], default: preset.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: {
    token: 'alice', onapply: apply,
    imageSources: [{ id: 'logo', kind: 'source', filename: 'logo.png', url: '/logo.png', createdAt: 0, mediaType: 'image' }],
  } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  const change = async (label: string, value: string) => {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value; control.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  }
  await change('Изображение в пресете Filmcraft', 'on')
  expect(button('Применить правки к экспорту').disabled).toBe(true)
  await change('Источник изображения пресета Filmcraft', 'logo')
  await change('Изображение: положение в пресете Filmcraft', 'bottomLeft')
  await input('Изображение: непрозрачность в пресете Filmcraft', '40')
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenCalledWith(preset.name, { effects: {
    image_overlay: { enabled: true, source_id: 'logo', path: '', placement: 'bottomLeft', opacity: 40 },
  } })
})

it('updates a user preset in place and keeps its category and description', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [user], default: user.name })
  vi.mocked(editFilmcraftExportPresets).mockResolvedValue({ name: user.name })
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: vi.fn() } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Сохранить изменения пресета').disabled).toBe(true))
  await input('Битрейт нового пресета Filmcraft', '1500')
  button('Сохранить изменения пресета').click()
  await vi.waitFor(() => expect(editFilmcraftExportPresets).toHaveBeenCalledWith('alice', {
    action: 'save', name: user.name, from: user.name, category: user.category,
    description: user.description, settings: { bitrateKbps: 1500, adaptiveBitrate: null }, overwrite: true,
  }, expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Применить правки к экспорту').disabled).toBe(true))
})

it('switches formats and resolves the rate mode and the MXF codec', async () => {
  const preset = { ...builtin, settings: { format: 'h264', bitrateMode: 'vbr2Pass', proresProfile: 'lt' } }
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [preset], default: preset.name })
  const apply = vi.fn()
  component = mount(Library, { target: document.body, props: { token: 'alice', onapply: apply } })
  await tick()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  const select = async (label: string, value: string) => {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value
    control.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
  }
  await select('Формат пресета Filmcraft', 'hevc')
  expect(document.querySelector('[aria-label="Профиль H.264 пресета Filmcraft"]')).toBeNull()
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenLastCalledWith(preset.name, { format: 'hevc', bitrateMode: 'vbr1Pass' })

  await select('Формат пресета Filmcraft', 'mxf-op1a')
  await select('Кодек MXF пресета Filmcraft', 'proRes')
  button('Применить правки к экспорту').click()
  expect(apply).toHaveBeenLastCalledWith(preset.name, { format: 'mxf-op1a', mxfVideoCodec: 'proRes', proresProfile: 'hq' })
})
