// @ts-expect-error Tests use the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { getFilmcraftPreviewFrame, listFilmcraftAnalysis } from '$lib/api'
import Preview from './FilmcraftProgramPreview.svelte'
vi.mock('$lib/api', () => ({ getFilmcraftPreviewFrame: vi.fn(), listFilmcraftAnalysis: vi.fn().mockResolvedValue({ jobs: [], nextCursor: null }), startFilmcraftAnalysis: vi.fn(), cancelFilmcraftAnalysis: vi.fn(), getFilmcraftAnalysis: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
const frame = { kind: 'sequence' as const, sequenceId: '18446744073709551615', scale: 1, effects: true, captions: true, width: 1, height: 1, ptsTicks: '9007199254740993', rgbaHex: 'ff008080' }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.restoreAllMocks(); vi.resetAllMocks(); document.body.innerHTML = '' })
function setup(timeFrames = '9007199254740993', sources: Record<string, string> = {}) {
 vi.mocked(listFilmcraftAnalysis).mockResolvedValue({ jobs: [], nextCursor: null })
 vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ createImageData: () => ({ data: new Uint8ClampedArray(4) }), putImageData: vi.fn() } as unknown as CanvasRenderingContext2D)
 component = mount(Preview, { target: document.body, props: { document: '{"project":{}}', sequenceId: frame.sequenceId, timeFrames, token: 'owner', sources } })
}
const checkbox = (n: number) => document.querySelectorAll<HTMLInputElement>('input[type=checkbox]')[n]!
it('requests composed pixels with exact string ids/time only when enabled', async () => {
 vi.mocked(getFilmcraftPreviewFrame).mockResolvedValue(frame); setup(); await tick(); expect(getFilmcraftPreviewFrame).not.toHaveBeenCalled(); checkbox(0).click()
 await vi.waitFor(() => expect(document.querySelector('canvas')).not.toBeNull())
 expect(getFilmcraftPreviewFrame).toHaveBeenCalledWith({ document: '{"project":{}}', sequenceId: frame.sequenceId, timeFrames: '9007199254740993', effects: true, captions: true, sources: {} }, expect.any(AbortSignal), 'owner'); expect(document.body.textContent).toContain('Кадр композиции'); expect(document.body.textContent).toContain(frame.ptsTicks)
})
it('aborts when disabled and suppresses a late frame', async () => {
 let finish!: (value: typeof frame) => void; vi.mocked(getFilmcraftPreviewFrame).mockImplementation(() => new Promise(resolve => { finish = resolve })); setup(); checkbox(0).click()
 await vi.waitFor(() => expect(getFilmcraftPreviewFrame).toHaveBeenCalledTimes(1)); const signal = vi.mocked(getFilmcraftPreviewFrame).mock.calls[0]![1]!; checkbox(0).click(); await tick(); expect(signal.aborted).toBe(true); finish(frame); await tick(); expect(document.querySelector('canvas')).toBeNull()
})
it('shows source failures and retries without a substitute frame', async () => {
 vi.mocked(getFilmcraftPreviewFrame).mockRejectedValueOnce(new Error('file source provider is not connected')).mockResolvedValue(frame); setup(); checkbox(0).click()
 await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('file source provider')); expect(document.querySelector('canvas')).toBeNull(); Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(button => button.textContent === 'Повторить предпросмотр')!.click(); await vi.waitFor(() => expect(document.querySelector('canvas')).not.toBeNull()); expect(getFilmcraftPreviewFrame).toHaveBeenCalledTimes(2)
})
it('refreshes effect switches and refuses invalid frame clocks', async () => {
 vi.mocked(getFilmcraftPreviewFrame).mockResolvedValue(frame); setup(); checkbox(0).click(); await vi.waitFor(() => expect(getFilmcraftPreviewFrame).toHaveBeenCalledTimes(1)); checkbox(1).click(); await vi.waitFor(() => expect(getFilmcraftPreviewFrame).toHaveBeenCalledTimes(2)); expect(vi.mocked(getFilmcraftPreviewFrame).mock.calls[1]![0].effects).toBe(false)
 await unmount(component!); component = undefined; document.body.innerHTML = ''; setup('9223372036854775808'); checkbox(0).click(); await tick(); expect(getFilmcraftPreviewFrame).toHaveBeenCalledTimes(2)
})

it('passes only nonempty library bindings with exact source item ids', async () => {
 vi.mocked(getFilmcraftPreviewFrame).mockResolvedValue(frame); setup('0', { '18446744073709551614': 'image-source', '2': '' }); checkbox(0).click(); await vi.waitFor(() => expect(getFilmcraftPreviewFrame).toHaveBeenCalledTimes(1)); expect(vi.mocked(getFilmcraftPreviewFrame).mock.calls[0]![0].sources).toEqual({ '18446744073709551614': 'image-source' })
})
