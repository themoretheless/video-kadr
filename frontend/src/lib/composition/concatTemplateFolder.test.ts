import { expect, it } from 'vitest'
import { readConcatTemplateFolder } from './concatTemplateFolder'

function file(path: string, text = '') {
  const selected = new File([text], path.split('/').at(-1)!)
  Object.defineProperty(selected, 'webkitRelativePath', { value: `Design/${path}` })
  return selected
}

it('resolves design media and fonts while preserving a placeholder with no file', async () => {
  const manifest = file('template.json', JSON.stringify({
    media: [
      { id: 'design', kind: 'image', path: 'assets/picture.png' },
      { id: 'slot', kind: 'video', placeholder: true, path: '' },
    ],
    fonts: [{ family: 'Title', path: 'assets/title.otf' }],
  }))
  const picture = file('assets/picture.png')
  const font = file('assets/title.otf')
  const folder = await readConcatTemplateFolder([manifest, picture, font])
  expect(folder.design).toEqual([{ id: 'design', kind: 'image', file: picture }])
  expect(folder.media[1]).toMatchObject({ id: 'slot', placeholder: true })
  expect(folder.fonts[0]?.file).toBe(font)
})

it.each(['assets/../outside.png', '/absolute.png', 'assets/missing.png'])(
  'rejects a missing or external design resource at %s',
  async path => {
    const manifest = file('template.json', JSON.stringify({ media: [{ id: 'picture', path }] }))
    await expect(readConcatTemplateFolder([manifest])).rejects.toThrow()
  },
)

it('uses the active timeline and sums editorial durations of repeated slots', async () => {
  const { concatTemplateSlotStats } = await import('./concatTemplateFolder')
  const stats = concatTemplateSlotStats({
    activeTimelineId: 'selected',
    timelines: [
      { id: 'other', clips: [{ mediaId: 'slot', start: 0, duration: 99 }] },
      { id: 'selected', clips: [
        { mediaId: 'slot', start: 7, duration: 2 },
        { mediaId: 'slot', start: 2, duration: 3 },
      ] },
    ],
  })
  expect(stats.get('slot')).toEqual({ first: 2, seconds: 5 })
  expect(stats.has('unused')).toBe(false)
})
