// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { replaceFilmcraftClips, type FilmcraftReplaceResult } from '$lib/api'
import Controls from './FilmcraftReplaceControls.svelte'
vi.mock('$lib/api', () => ({ replaceFilmcraftClips: vi.fn() }))
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '9007199254740993', token: 'owner', selection: ['9007199254740993'], projectSelection: ['9007199254740995'], sourceItem: '9007199254740995', sourcePlayhead: '9007199254740997', timeFrames: '7', timeTicks: '9007199254740999', items: [{ id: '9007199254740995', name: 'Source', kind: 'Media' as const }] }
const result = { documentText: 'changed', replacementEdit: true, replacedClipIds: props.selection, replacementItemId: props.sourceItem, sourceInClamped: props.selection, shortClips: [{ clip: props.clipId, shortByFrames: '13' }], originalSchema: 12, sequences: [], media: [] } as FilmcraftReplaceResult
let component: ReturnType<typeof mount> | undefined
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each([['Заменить из Source Monitor', 'clip.replaceFromSource'], ['Заменить с совмещением кадра', 'clip.replaceFromSourceMatchFrame'], ['Заменить из проекта', 'clip.replaceFromBin']] as const)('runs %s with exact state fences and reports media limits', async (label, command) => {
  vi.mocked(replaceFilmcraftClips).mockResolvedValue(result); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button(label).click(); await tick(); await tick()
  expect(replaceFilmcraftClips).toHaveBeenCalledWith('owner', command, { document: props.document, sequence: props.sequenceId, clips: props.selection, ...(command === 'clip.replaceFromBin' ? { keepSourceIn: false } : {}), context: { activeSequence: props.sequenceId, selection: props.selection, projectSelection: props.projectSelection, sourceItem: props.sourceItem, sourcePlayhead: props.sourcePlayhead, playhead: props.timeTicks, linkedSelection: true } }, expect.any(AbortSignal))
  expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, timelineSelection: props.selection, projectSelection: props.projectSelection, sourceItem: props.sourceItem, sourcePlayhead: props.sourcePlayhead, timeFrames: props.timeFrames, timeTicks: props.timeTicks })
  expect(document.body.textContent).toContain('не хватает 13 кадров'); expect(document.body.textContent).toContain('Source In сдвинут')
})
it('rejects late replacement when selection changes', async () => {
  let resolve!: (result: FilmcraftReplaceResult) => void
  vi.mocked(replaceFilmcraftClips).mockImplementation(() => new Promise(done => { resolve = done })); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button('Заменить из Source Monitor').click(); await tick()
  props.selection.push('9007199254740997'); resolve(result); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled(); props.selection.pop()
})
