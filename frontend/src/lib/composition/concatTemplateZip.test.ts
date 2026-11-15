import { TextReader, Uint8ArrayWriter, ZipWriter } from '@zip.js/zip.js'
import { expect, it } from 'vitest'
import { readConcatTemplateZip } from './concatTemplateZip'

it('rejects traversal entries before handing any ZIP content to the importer', async () => {
  const writer = new ZipWriter(new Uint8ArrayWriter(), { useWebWorkers: false })
  await writer.add('template.json', new TextReader('{}'), { level: 0 })
  await writer.add('assets/../private.txt', new TextReader('outside'), { level: 0 })
  const file = new File([new Uint8Array(await writer.close())], 'template.zip')
  await expect(readConcatTemplateZip(file, new AbortController().signal))
    .rejects.toThrow(/Некорректный путь|Unsafe filename/)
})
