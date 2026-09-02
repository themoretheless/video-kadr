import { chromium } from '@playwright/test'
import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { extname, join } from 'node:path'

const root = new URL('./', import.meta.url).pathname
const server = createServer(async (request, response) => {
  const filename = request.url === '/' ? 'benchmark.html' : request.url.slice(1)
  const body = await readFile(join(root, filename))
  response.setHeader('content-type', extname(filename) === '.js' ? 'text/javascript' : 'text/html')
  response.end(body)
})
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
const address = server.address()
if (!address || typeof address === 'string') throw new Error('Benchmark server did not bind')

const browser = await chromium.launch({ headless: true })
try {
  const page = await browser.newPage()
  await page.goto(`http://127.0.0.1:${address.port}`)
  const results = await page.evaluate(() => globalThis.canvasBenchmark)
  console.log(JSON.stringify({ workload: 'long-timeline-v1', results }, null, 2))
} finally {
  await browser.close()
  server.close()
}
