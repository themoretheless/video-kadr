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
