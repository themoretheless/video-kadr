import { TextReader, Uint8ArrayWriter, ZipWriter } from '@zip.js/zip.js'
import { expect, it } from 'vitest'
import { readConcatTemplateCaptions } from './concatTemplateCaptions'
import { readConcatTemplateFolder } from './concatTemplateFolder'
import { readConcatTemplateZip } from './concatTemplateZip'

it('rejects traversal entries before handing any ZIP content to the importer', async () => {
  const writer = new ZipWriter(new Uint8ArrayWriter(), { useWebWorkers: false })
  await writer.add('template.json', new TextReader('{}'), { level: 0 })
  await writer.add('assets/../private.txt', new TextReader('outside'), { level: 0 })
  const file = new File([new Uint8Array(await writer.close())], 'template.zip')
  await expect(readConcatTemplateZip(file, new AbortController().signal))
    .rejects.toThrow(/Некорректный путь|Unsafe filename/)
})

it('opens an archived template directory without mixing in sibling assets', async () => {
  const writer = new ZipWriter(new Uint8ArrayWriter(), { useWebWorkers: false })
  const manifest = JSON.stringify({ media: [{ id: 'design', kind: 'image', path: 'assets/design.png' }] })
  await writer.add('Design/template.json', new TextReader(manifest))
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(manifest))
  const manifestSha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('')
  const clips = [['TL1', { id: 'cue', text: 'Text' }]]
  await writer.add('Design/video-kadr-caption-clips.json', new TextReader(JSON.stringify({ manifestSha256, clips })))
  await writer.add('Design/assets/design.png', new TextReader('bundled'))
  await writer.add('Design/poster.jpg', new TextReader('poster'))
  const subtitles = [{ id: 'cue', name: 'cue.srt', text: '1\n00:00:00,000 --> 00:00:01,000\nТекст' }]
  await writer.add('Design/video-kadr-subtitles.json', new TextReader(JSON.stringify(subtitles)))
  await writer.add('assets/design.png', new TextReader('unrelated'))
  const zip = new File([new Uint8Array(await writer.close())], 'Design.zip')
  const selected = await readConcatTemplateZip(zip, new AbortController().signal)
  const folder = await readConcatTemplateFolder(selected)
  expect(await folder.design[0]?.file.text()).toBe('bundled')
  expect(await folder.poster?.text()).toBe('poster')
  expect(folder.subtitleDocuments).toEqual(subtitles)
  expect(folder.captionClips).toEqual(clips)
  expect(selected).toHaveLength(5)
  const sidecar = selected.find(file => file.name === 'video-kadr-caption-clips.json')!
  await expect(readConcatTemplateCaptions(sidecar, new File([manifest + ' '], 'template.json'))).resolves.toBeUndefined()
})

it('rejects a ZIP with two candidate template directories', async () => {
  const writer = new ZipWriter(new Uint8ArrayWriter(), { useWebWorkers: false })
  await writer.add('One/template.json', new TextReader('{}'))
  await writer.add('Two/template.json', new TextReader('{}'))
  const zip = new File([new Uint8Array(await writer.close())], 'Templates.zip')
  await expect(readConcatTemplateZip(zip, new AbortController().signal)).rejects.toThrow('один шаблон')
})
