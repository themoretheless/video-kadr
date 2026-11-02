// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftAttributes, type FilmcraftAttributesResult } from '$lib/api'
import Controls from './FilmcraftAttributesControls.svelte'
vi.mock('$lib/api', () => ({ editFilmcraftAttributes: vi.fn(), listFilmcraftEffects: vi.fn() }))
const props = { document: '{"id":18446744073709551615}', sequenceId: '18446744073709551615', clipId: '9007199254740993', token: 'owner', selection: ['9007199254740993'], linked: true, clipboardText: 'opaque clipboard' }
const result = { documentText: props.document, attributeEdit: true, changedClipCount: 0, originalSchema: 12, sequences: [], media: [] } as FilmcraftAttributesResult
let component: ReturnType<typeof mount> | undefined
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each([['Вставить атрибуты', 'edit.pasteAttributes'], ['Удалить атрибуты', 'edit.removeAttributes']] as const)('runs %s with exact context, preferences and accepted no-op adoption', async (label, command) => {
  vi.mocked(editFilmcraftAttributes).mockResolvedValue(result)
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button(label).click(); await tick(); await tick()
  expect(editFilmcraftAttributes).toHaveBeenCalledWith('owner', command, { document: props.document, sequence: props.sequenceId, clips: props.selection, ...(command === 'edit.pasteAttributes' ? { clipboardText: props.clipboardText } : {}), motion: true, opacity: true, timeRemapping: true, volume: true, channelVolume: true, panner: true, scaleTimes: true, effects: true, context: { activeSequence: props.sequenceId, selection: props.selection, linkedSelection: true } }, expect.any(AbortSignal))
  expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, timelineSelection: props.selection })
})
it('does not apply a pending response after selection changes', async () => {
  let resolve!: (result: FilmcraftAttributesResult) => void
  vi.mocked(editFilmcraftAttributes).mockImplementation(() => new Promise(done => { resolve = done }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button('Вставить атрибуты').click(); await tick()
  props.selection.push('9007199254740995')
  resolve(result); await tick(); await tick()
  expect(onedit).not.toHaveBeenCalled()
  props.selection.pop()
})
it('blocks paste without a clipboard and both edits without a selection', async () => {
  component = mount(Controls, { target: document.body, props: { ...props, selection: [], clipboardText: undefined, onedit: vi.fn() } }); await tick()
  expect(button('Вставить атрибуты').disabled).toBe(true)
  expect(button('Удалить атрибуты').disabled).toBe(true)
})
