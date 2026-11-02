import { spawn } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { mkdtemp, rm, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { chromium, expect } from '@playwright/test'
import { createServer as createNetServer } from 'node:net'
import assert from 'node:assert/strict'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
async function freePort() {
  const server = createNetServer()
  await new Promise((done) => server.listen(0, '127.0.0.1', done))
  const port = server.address().port
  await new Promise((done) => server.close(done))
  return port
}
const backendPort = await freePort()
const frontendPort = await freePort()
const backendUrl = `http://127.0.0.1:${backendPort}`
const frontendUrl = `http://127.0.0.1:${frontendPort}`
const storage = await mkdtemp(join(tmpdir(), 'video-kadr-agent-e2e-'))
const token = randomBytes(32).toString('hex')
await mkdir(join(storage, 'sources'))
const sceneSource = { id: 'scene-e2e', kind: 'source', filename: 'scene.mkv', url: '/files/sources/scene.mkv', mediaType: 'video', duration: 2, width: 320, height: 180, fps: 25, vcodec: 'ffv1', createdAt: 0 }
try {
  const fixture = spawn('ffmpeg', ['-v', 'error', '-f', 'lavfi', '-i', 'color=black:s=320x180:r=25:d=1', '-f', 'lavfi', '-i', 'color=white:s=320x180:r=25:d=1', '-filter_complex', '[0:v][1:v]concat=n=2:v=1:a=0', '-c:v', 'ffv1', '-y', join(storage, 'sources/scene.mkv')], { stdio: 'ignore' })
  await new Promise((done, fail) => { fixture.once('error', fail); fixture.once('exit', (code) => code === 0 ? done() : fail(new Error(`Fixture FFmpeg exited ${code}`))) })
  await writeFile(join(storage, 'library.json'), JSON.stringify([sceneSource]))
} catch (error) { await rm(storage, { recursive: true, force: true }); throw error }
let logs = ''
const backend = spawn(resolve(root, '../backend/target/debug/video-kadr-backend'), [], {
  cwd: root,
  env: { ...process.env, STORAGE_DIR: storage, PORT: String(backendPort), BIND_ADDR: '127.0.0.1', CORS_ALLOW_ORIGINS: frontendUrl, VIDEO_KADR_MCP_TOKEN: token },
  stdio: ['ignore', 'pipe', 'pipe'],
})
backend.stdout.on('data', (data) => { logs = (logs + data).slice(-8000) })
backend.stderr.on('data', (data) => { logs = (logs + data).slice(-8000) })
let server
let browser
qualification: try {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (backend.exitCode !== null) throw new Error(`Backend exited: ${logs}`)
    try { if ((await fetch(`${backendUrl}/api/health`)).ok) break } catch { /* startup */ }
    if (attempt === 99) throw new Error(`Backend not ready: ${logs}`)
    await new Promise((done) => setTimeout(done, 100))
  }
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), server: { host: '127.0.0.1', port: frontendPort, strictPort: true, watch: null, hmr: false, proxy: { '/api': backendUrl, '/files': backendUrl } } })
  await server.listen()
  browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) })
  const page = await browser.newPage()
  await page.goto(frontendUrl)
  await page.getByRole('button', { name: 'Multitrack', exact: true }).click()
  await page.locator('summary', { hasText: 'Агент и история' }).click()
  await page.getByLabel('Ключ подключения').fill(token)
  await page.getByRole('button', { name: 'Подключить агента', exact: true }).click()
  for (let attempt = 0; attempt < 50; attempt++) {
    const status = await fetch(`${backendUrl}/api/agent/status`, { headers: { Authorization: `Bearer ${token}` } }).then((response) => response.json())
    if (status.connected) break
    if (attempt === 49) throw new Error('Editor never connected')
    await new Promise((done) => setTimeout(done, 100))
  }
  const initialized = await fetch(`${backendUrl}/mcp`, { method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 'initialize', method: 'initialize', params: { protocolVersion: '2025-03-26', capabilities: {}, clientInfo: { name: 'e2e', version: '1' } } }) })
  const sessionId = initialized.headers.get('mcp-session-id')
  assert.ok(sessionId)
  let callId = 0
  async function tool(name, args = {}) {
    const response = await fetch(`${backendUrl}/mcp`, { method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', 'Mcp-Session-Id': sessionId }, body: JSON.stringify({ jsonrpc: '2.0', id: ++callId, method: 'tools/call', params: { name, arguments: args } }) })
    assert.equal(response.status, 200)
    const rpc = await response.json()
    assert.equal(rpc.id, callId)
    return { failed: rpc.result.isError, data: JSON.parse(rpc.result.content[0].text) }
  }
  const project = await tool('get_project')
  assert.equal(project.failed, false)
  const checkpoint = await tool('create_checkpoint', { label: 'Before MCP' })
  const operations = [
    { op: 'add_track', track: { id: 'mcp-title-track', kind: 'text', name: 'MCP captions', locked: false, hidden: false, clips: [] } },
    { op: 'add_layer', trackId: 'mcp-title-track', clip: { id: 'mcp-title', kind: 'text', timelineStartTicks: 0, durationTicks: 2_000_000, text: 'MCP live title', x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } } },
  ]
  assert.equal((await tool('dry_run', { expectedRevision: project.data.revision, operations })).failed, false)
  assert.equal((await tool('get_project')).data.document.tracks.length, 0)
  assert.equal((await tool('apply_batch', { expectedRevision: project.data.revision, operations })).failed, false)
  await page.locator('.composition-clip.text').waitFor()
  assert.equal(await page.locator('.composition-clip.text').count(), 1)
  assert.equal((await tool('get_history')).data.undoCount, 1)
  // Color commands cross the real HTTP MCP session and mounted browser editor.
  {
    const colorRequests = []
    const observeColor = request => { if (request.url().endsWith('/api/compositions/animated-color')) colorRequests.push(request) }
    page.on('request',observeColor)
    const before = (await tool('get_project')).data
    const depth = (await tool('get_history')).data.undoCount
    const colorOps = [
      { op: 'set_color_key',clipId: 'mcp-title',playheadTicks: 500000,value: { r: 255,g: 0,b: 0,a: 128 } },
      { op: 'set_color_key',clipId: 'mcp-title',playheadTicks: 1000000,value: { r: 0,g: 0,b: 255,a: 255 } },
      { op: 'set_color_interpolation',clipId: 'mcp-title',tick: 500000,interpolation: { kind: 'bezier',p1: [0.2,-0.3],p2: [0.8,1.4] } },
      { op: 'set_color_extrapolation',clipId: 'mcp-title',side: 'after',extrapolation: 'ping_pong' },
    ]
    assert.equal((await tool('dry_run',{ expectedRevision: before.revision,operations: colorOps })).failed,false)
    assert.deepEqual((await tool('get_project')).data.document,before.document)
    assert.equal((await tool('apply_batch',{ expectedRevision: before.revision,operations: colorOps })).failed,false)
    assert.equal((await tool('get_history')).data.undoCount,depth+1)
    const read = await tool('get_color_track',{ clipId: 'mcp-title' })
    assert.equal(read.failed,false)
    assert.deepEqual(read.data.keyframes[1].value,{ r: 255,g: 0,b: 0,a: 128 })
    assert.equal(read.data.keyframes[1].outgoing.x,0.2)
    assert.equal(read.data.keyframes[2].incoming.y,1.4)
    assert.equal(read.data.extrapolation.after,'ping_pong')
    // Preview must consume actual native HTTP output at a non-key time.
    const colorResponse = page.waitForResponse(response => response.url().endsWith('/api/compositions/animated-color') && response.request().method() === 'POST')
    await page.evaluate(async () => {
      const { setCompositionPlayhead } = await import('/src/lib/state/composition/transport.svelte.ts')
      const { sampleAnimatedColors } = await import('/src/lib/api.ts')
      const { packColorAnimation,colorToNative } = await import('/src/lib/composition/colorAnimation.ts')
      const { compositionState } = await import('/src/lib/state/composition/core.svelte.ts')
      const clip = compositionState.document.tracks.flatMap(track => track.clips).find(clip => clip.id === 'mcp-title')
      await sampleAnimatedColors({ track: packColorAnimation(clip.colorAnimation),default: colorToNative(clip.style.color),times_us: [750000] })
      setCompositionPlayhead(750000)
    })
    const sampled = await colorResponse
    assert.equal(sampled.status(),200)
    const { colors } = await sampled.json()
    const rgba = colors[0]
    assert.ok(rgba && rgba.r > 0 && rgba.b > 0)
    const localColor = await page.evaluate(async () => {
      const { prepareColorPreview } = await import('/src/lib/native/colorPreview.ts')
      const { packColorAnimation,colorToNative } = await import('/src/lib/composition/colorAnimation.ts')
      const { compositionState } = await import('/src/lib/state/composition/core.svelte.ts')
      const clip = compositionState.document.tracks.flatMap(track => track.clips).find(clip => clip.id === 'mcp-title')
      const sample = await prepareColorPreview(packColorAnimation(clip.colorAnimation),colorToNative(clip.style.color))
      const first = sample(750000)
      const other = await prepareColorPreview({ keys: [],extrapolate: { before: 'Hold',after: 'Hold' } },{ r: 0,g: 255,b: 0,a: 255 })
      return { first,again: sample(750000),other: other(750000) }
    })
    assert.deepEqual(localColor.first,rgba)
    assert.deepEqual(localColor.again,rgba)
    assert.deepEqual(localColor.other,{ r: 0,g: 255,b: 0,a: 255 })
    const expectedColor = await page.evaluate(color => {
      const element = document.createElement('span')
      element.style.color = `rgba(${color.r},${color.g},${color.b},${color.a/255})`
      document.body.append(element)
      const result = getComputedStyle(element).color
      element.remove()
      return result
    },rgba)
    const textLayer = page.locator('.composition-text-layer').filter({ hasText: 'MCP live title' })
    await expect(textLayer).toBeVisible()
    await expect(textLayer).toHaveCSS('color',expectedColor)
    assert.equal(colorRequests.length,1,'Only explicit HTTP reference sampling is allowed; preview must use WASM')
    await page.evaluate(async () => {
      const { setCompositionPlayhead } = await import('/src/lib/state/composition/transport.svelte.ts')
      setCompositionPlayhead(0)
    })
    await page.getByRole('button',{ name: '▶ Играть',exact: true }).click()
    const playback = await page.evaluate(async () => {
      const { compositionState } = await import('/src/lib/state/composition/core.svelte.ts')
      const { prepareColorPreview } = await import('/src/lib/native/colorPreview.ts')
      const { packColorAnimation,colorToNative,nativeColorToHex } = await import('/src/lib/composition/colorAnimation.ts')
      const clip = compositionState.document.tracks.flatMap(track => track.clips).find(clip => clip.id === 'mcp-title')
      const sample = await prepareColorPreview(packColorAnimation(clip.colorAnimation),colorToNative(clip.style.color))
      const normalized = document.createElement('span')
      const frames = []
      const started = performance.now()
      while (compositionState.transport.playing && performance.now()-started < 3000) {
        await new Promise(resolve => requestAnimationFrame(resolve))
        await Promise.resolve()
        const time = compositionState.transport.playheadTicks
        const layer = [...document.querySelectorAll('.composition-text-layer')].find(layer => layer.textContent === 'MCP live title')
        normalized.style.color = nativeColorToHex(sample(time))
        frames.push({ time,visible: !!layer && getComputedStyle(layer).visibility !== 'hidden',correct: layer?.style.color === normalized.style.color })
        if (time >= 750000) break
      }
      return { frames,elapsedMs: performance.now()-started }
    })
    await page.getByRole('button',{ name: '⏸ Пауза',exact: true }).click()
    assert.ok(new Set(playback.frames.map(frame => frame.time)).size >= 5,'Playback must advance across frames')
    assert.ok(playback.frames.every(frame => frame.visible && frame.correct),'Every observed playback frame must display the current native color')
    assert.equal(colorRequests.length,1,'Playback must not sample colors over HTTP')
    console.log(`WASM color playback: ${playback.frames.length} observed frames in ${Math.round(playback.elapsedMs)}ms, current colors correct and visible, zero preview HTTP requests.`)
    page.off('request',observeColor)
    const beforeDrag = (await tool('get_project')).data.document
    const dragDepth = (await tool('get_history')).data.undoCount
    const colorKey = page.getByRole('button',{ name: 'Ключ цвета mcp-title 500000',exact: true })
    await colorKey.scrollIntoViewIfNeeded()
    const keyBounds = await colorKey.boundingBox()
    assert.ok(keyBounds)
    const zoom = await page.evaluate(async () => (await import('/src/lib/state/composition/core.svelte.ts')).compositionState.ui.zoomPxPerSecond)
    await page.mouse.move(keyBounds.x+keyBounds.width/2,keyBounds.y+keyBounds.height/2)
    await page.mouse.down()
    await page.mouse.move(keyBounds.x+keyBounds.width/2+zoom*0.25,keyBounds.y+keyBounds.height/2,{ steps: 4 })
    assert.deepEqual((await tool('get_project')).data.document,beforeDrag)
    await page.mouse.up()
    const moved = await tool('get_color_track',{ clipId: 'mcp-title' })
    assert.equal(moved.data.keyframes[1].tick,750000)
    assert.equal((await tool('get_history')).data.undoCount,dragDepth+1)
    assert.equal((await tool('undo')).failed,false)
    assert.deepEqual((await tool('get_project')).data.document,beforeDrag)
    await page.getByRole('button',{ name: 'Ключ цвета mcp-title 500000',exact: true }).click()
    const scaleKey = page.getByRole('button',{ name: 'Ключ цвета mcp-title 1000000',exact: true })
    await scaleKey.click({ modifiers: ['Shift'] })
    assert.equal(await page.locator('.color-key[aria-pressed="true"]').count(),2)
    const scaleBounds = await scaleKey.boundingBox()
    assert.ok(scaleBounds)
    await page.keyboard.down('Alt')
    await page.mouse.move(scaleBounds.x+scaleBounds.width/2,scaleBounds.y+scaleBounds.height/2)
    await page.mouse.down()
    await page.mouse.move(scaleBounds.x+scaleBounds.width/2+zoom*0.25,scaleBounds.y+scaleBounds.height/2,{ steps: 4 })
    assert.deepEqual((await tool('get_project')).data.document,beforeDrag)
    await page.mouse.up()
    await page.keyboard.up('Alt')
    const scaled = await tool('get_color_track',{ clipId: 'mcp-title' })
    assert.equal(scaled.data.keyframes[1].tick,500000)
    assert.equal(scaled.data.keyframes[2].tick,1250000)
    assert.equal((await tool('get_history')).data.undoCount,dragDepth+1)
    assert.equal((await tool('undo')).failed,false)
    assert.deepEqual((await tool('get_project')).data.document,beforeDrag)
    await page.getByRole('button',{ name: 'Ключ цвета mcp-title 500000',exact: true }).click()
    await page.getByRole('button',{ name: 'Ключ цвета mcp-title 1000000',exact: true }).click({ modifiers: ['Shift'] })
    await page.keyboard.press('Control+c')
    await page.evaluate(async () => {
      const { setCompositionPlayhead } = await import('/src/lib/state/composition/transport.svelte.ts')
      setCompositionPlayhead(1500000)
    })
    await page.getByRole('button',{ name: 'Ключ цвета mcp-title 500000',exact: true }).focus()
    await page.keyboard.press('Control+v')
    const pastedColors = await tool('get_color_track',{ clipId: 'mcp-title' })
    assert.deepEqual(pastedColors.data.keyframes.map(key => key.tick),[0,500000,1000000,1500000,2000000])
    assert.deepEqual(pastedColors.data.keyframes[3].value,{ r: 255,g: 0,b: 0,a: 128 })
    assert.equal((await tool('get_history')).data.undoCount,dragDepth+1)
    assert.equal((await tool('undo')).failed,false)
    assert.deepEqual((await tool('get_project')).data.document,beforeDrag)
    const changed = (await tool('get_project')).data
    assert.equal((await tool('apply_batch',{ expectedRevision: changed.revision,operations: [
      { op: 'delete_color_key',clipId: 'mcp-title',tick: 500000 },
      { op: 'set_color_key',clipId: 'mcp-title',playheadTicks: 500000,value: { r: 256,g: 0,b: 0,a: 255 } },
    ] })).failed,true)
    assert.deepEqual((await tool('get_project')).data.document,changed.document)
    assert.equal((await tool('undo')).failed,false)
    assert.deepEqual((await tool('get_project')).data.document,before.document)
    await page.evaluate(async () => {
      const { setCompositionPlayhead } = await import('/src/lib/state/composition/transport.svelte.ts')
      setCompositionPlayhead(0)
    })
    console.log('Real MCP → browser color batch, RGBA readback, Bezier handles, extrapolation, actual Rust HTTP seek preview, dry-run, rollback and Undo passed.')
  }
  if (process.env.VIDEO_KADR_E2E_COLOR_ONLY === '1') break qualification
  await page.keyboard.press('Control+k')
  const search = page.getByRole('combobox', { name: 'Команды, медиа, клипы, субтитры и маркеры' })
  await search.fill('MCP live title')
  await page.getByRole('option', { name: /Субтитр · MCP live title/ }).waitFor()
  await search.press('Enter')
  assert.equal(await page.getByRole('dialog', { name: 'Поиск в редакторе' }).isVisible(), false)
  await page.keyboard.press('Control+k')
  await search.fill('не существующий результат')
  await page.getByText('Ничего не найдено', { exact: true }).waitFor()
  await search.press('Escape')
  assert.equal(await page.getByRole('dialog', { name: 'Поиск в редакторе' }).isVisible(), false)
  assert.equal((await tool('apply_batch', { expectedRevision: project.data.revision, operations })).failed, true)
  assert.equal((await tool('undo')).failed, false)
  assert.equal((await tool('get_project')).data.document.tracks.length, 0)
  assert.equal((await tool('redo')).failed, false)
  assert.equal((await tool('restore_checkpoint', { id: checkpoint.data.id })).failed, false)
  assert.equal((await tool('get_project')).data.document.tracks.length, 0)
  const resource = await fetch(`${backendUrl}/mcp`, { method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', 'Mcp-Session-Id': sessionId }, body: JSON.stringify({ jsonrpc: '2.0', id: 'read', method: 'resources/read', params: { uri: 'project://document' } }) }).then((response) => response.json())
  assert.equal(JSON.parse(resource.result.contents[0].text).document.tracks.length, 0)
  const clipId = await page.evaluate(async (source) => {
    const { addLibraryEntryToComposition } = await import('/src/lib/state/composition/media.svelte.ts')
    return addLibraryEntryToComposition(source)
  }, sceneSource)
  const beforeShots = await tool('get_project')
  const historyBefore = (await tool('get_history')).data.undoCount
  const analyzed = await tool('analyze_clip', { clipId })
  assert.equal(analyzed.failed, false, JSON.stringify(analyzed))
  assert.equal(analyzed.data.rows.length, 2)
  assert.equal(analyzed.data.scan.cuts[0].tick, 1_000_000)
  assert.equal((await tool('get_history')).data.undoCount, historyBefore)
  const measured = await tool('measure_shots', { clipId })
  assert.equal(measured.failed, false, JSON.stringify(measured))
  assert.ok(measured.data.measurements[0].flags.includes('black'))
  assert.ok(measured.data.measurements[1].brightness > 0.99)
  const frames = await tool('compare_frames', { clipId, leftTick: 500_000, rightTick: 1_500_000 })
  assert.equal(frames.failed, false)
  assert.ok(frames.data.similarity < 0.1)
  await page.locator('summary', { hasText: 'Сцены и качество исходника' }).click()
  await page.getByRole('list', { name: 'Сцены выбранного клипа' }).locator('li').nth(1).waitFor()
  await page.getByRole('button', { name: 'Сравнить кадры', exact: true }).click()
  await page.getByRole('img', { name: 'Кадр после границы' }).waitFor()
  assert.equal(await page.getByRole('img', { name: 'Кадр перед границей' }).evaluate((image) => image.complete && image.naturalWidth > 0), true)
  const split = await tool('auto_split_by_shot', { clipId, expectedRevision: beforeShots.data.revision })
  assert.equal(split.failed, false, JSON.stringify(split))
  assert.equal((await tool('get_project')).data.document.tracks[0].clips.length, 2)
  assert.equal((await tool('get_history')).data.undoCount, historyBefore + 1)
  assert.equal((await tool('undo')).failed, false)
  assert.equal((await tool('get_project')).data.document.tracks[0].clips.length, 1)
  await page.getByRole('button', { name: 'Разделить по границам', exact: true }).click()
  await page.getByRole('status').filter({ hasText: 'Правка применена одной записью Undo' }).waitFor()
  assert.equal((await tool('get_project')).data.document.tracks[0].clips.length, 2)
  assert.equal((await tool('get_history')).data.undoCount, historyBefore + 1)
  assert.equal((await tool('undo')).failed, false)
  const linkBefore = await tool('get_project')
  // This fixture asserts half-second phases, canonical on the explicit 30 fps grid.
  const linkOperations = [{ op: 'update_canvas', patch: { fps: 30 } }, ...['A', 'B'].flatMap((label, index) => [
    { op: 'add_track', track: { id: `link-track-${label}`, kind: 'text', name: `Link ${label}`, locked: false, hidden: false, clips: [] } },
    { op: 'add_layer', trackId: `link-track-${label}`, clip: { id: `link-clip-${label}`, kind: 'text', timelineStartTicks: index * 500_000, durationTicks: 2_000_000, text: `Link ${label}`, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } } },
  ])]
  linkOperations.push({ op: 'add_track', track: { id: 'link-track-C', kind: 'text', name: 'Link C', locked: false, hidden: false, clips: [] } })
  assert.equal((await tool('apply_batch', { expectedRevision: linkBefore.data.revision, operations: linkOperations })).failed, false)
  await page.locator('summary', { hasText: 'Связи клипов' }).click()
  await page.getByRole('checkbox', { name: /Link A · Link A/ }).check()
  await page.getByRole('checkbox', { name: /Link B · Link B/ }).check()
  await page.getByRole('button', { name: 'Связать выбранные клипы', exact: true }).click()
  let linked = await tool('get_project')
  assert.deepEqual(linked.data.document.links[0].memberIds, ['link-clip-A', 'link-clip-B'])
  const clipA = page.getByRole('button', { name: 'Link A, 2.0s', exact: true })
  await clipA.click()
  await expect(page.locator('.composition-clip.text.selected')).toHaveCount(2)
  const toggleHistory = (await tool('get_history')).data.undoCount
  await page.getByRole('slider', { name: 'Плейхед на дорожке Link A', exact: true }).focus()
  await page.keyboard.press('Control+l')
  assert.equal((await tool('get_project')).data.document.links.length, 0)
  await page.evaluate(async () => {
    const { compositionState } = await import('/src/lib/state/composition.svelte.ts')
    compositionState.ui.snapEnabled = false
    compositionState.ui.selectedClipIds = []
    compositionState.ui.selectedClipId = null
    compositionState.ui.selectedTrackId = null
  })
  const marqueeBefore = (await tool('get_project')).data.document
  const marqueeHistory = (await tool('get_history')).data.undoCount
  const marqueeA = page.locator('[data-track-id="link-track-A"] .composition-clip.text')
  const marqueeB = page.locator('[data-track-id="link-track-B"] .composition-clip.text')
  await marqueeB.scrollIntoViewIfNeeded()
  const boxA = await marqueeA.boundingBox()
  const boxB = await marqueeB.boundingBox()
  assert.ok(boxA && boxB)
  const marqueeZoom = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.zoomPxPerSecond)
  await page.mouse.click(boxB.x - marqueeZoom * 0.25, boxB.y + 15)
  const blankClick = await page.evaluate(async () => {
    const { compositionState } = await import('/src/lib/state/composition.svelte.ts')
    return { gap: compositionState.ui.selectedGap, playhead: compositionState.transport.playheadTicks }
  })
  assert.equal(blankClick.gap?.trackId, 'link-track-B')
  assert.ok(blankClick.playhead > 0)
  const boxStart = { x: Math.max(boxA.x + boxA.width, boxB.x + boxB.width) + 12, y: boxB.y + boxB.height + 3 }
  const boxEnd = { x: Math.min(boxA.x, boxB.x) + 12, y: boxA.y + 2 }
  const selectionIds = () => page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedClipIds)
  for (const cancel of [true, false]) {
    await page.mouse.move(boxStart.x, boxStart.y)
    await page.mouse.down()
    await page.mouse.move(boxEnd.x, boxEnd.y, { steps: 5 })
    await expect(page.locator('.composition-marquee')).toHaveCount(1)
    assert.deepEqual(await selectionIds(), ['link-clip-A', 'link-clip-B'])
    if (cancel) {
      await page.mouse.move(boxEnd.x, boxB.y + 10)
      assert.deepEqual(await selectionIds(), ['link-clip-B'])
      await page.keyboard.press('Escape')
      assert.deepEqual(await selectionIds(), [])
    }
    await page.mouse.up()
    await expect(page.locator('.composition-marquee')).toHaveCount(0)
  }
  assert.deepEqual((await tool('get_project')).data.document, marqueeBefore)
  assert.equal((await tool('get_history')).data.undoCount, marqueeHistory)
  // Physical edge scrolling: content-anchored press, horizontal-only motion and cancellation.
  await page.evaluate(async () => {
    const { compositionState } = await import('/src/lib/state/composition.svelte.ts')
    compositionState.ui.zoomPxPerSecond = 500
  })
  const scrollArea = page.locator('.composition-timeline-scroll')
  await expect.poll(() => scrollArea.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(true)
  await scrollArea.evaluate((el) => { el.scrollLeft = 0 })
  const scrollBox = await scrollArea.boundingBox()
  const edgeClipBox = await marqueeB.boundingBox()
  assert.ok(scrollBox && edgeClipBox)
  const scrollSnapshot = await scrollArea.evaluate((el) => ({ left: el.scrollLeft, top: el.scrollTop }))
  await page.mouse.move(scrollBox.x + scrollBox.width - 80, edgeClipBox.y + edgeClipBox.height + 3)
  await page.mouse.down()
  await page.mouse.move(scrollBox.x + scrollBox.width - 3, edgeClipBox.y + 10, { steps: 3 })
  await expect.poll(() => scrollArea.evaluate((el) => el.scrollLeft)).toBeGreaterThan(scrollSnapshot.left + 40)
  assert.equal(await scrollArea.evaluate((el) => el.scrollTop), scrollSnapshot.top)
  await page.keyboard.press('Escape')
  await page.mouse.up()
  await expect(page.locator('.composition-marquee')).toHaveCount(0)
  const stoppedScroll = await scrollArea.evaluate((el) => el.scrollLeft)
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))
  assert.equal(await scrollArea.evaluate((el) => el.scrollLeft), stoppedScroll)
  assert.deepEqual(await selectionIds(), ['link-clip-A', 'link-clip-B'])
  assert.deepEqual((await tool('get_project')).data.document, marqueeBefore)
  assert.equal((await tool('get_history')).data.undoCount, marqueeHistory)
  await page.evaluate(async (zoom) => {
    const { compositionState } = await import('/src/lib/state/composition.svelte.ts')
    compositionState.ui.zoomPxPerSecond = zoom
  }, marqueeZoom)
  await scrollArea.evaluate((el) => { el.scrollLeft = 0 })
  const dragBefore = (await tool('get_project')).data.document
  const dragHistory = (await tool('get_history')).data.undoCount
  const dragZoom = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.zoomPxPerSecond)
  const dragAnchor = page.locator('[data-track-id="link-track-A"] .composition-clip.text')
  await dragAnchor.scrollIntoViewIfNeeded()
  const dragBox = await dragAnchor.boundingBox()
  assert.ok(dragBox)
  await page.mouse.move(dragBox.x + 12, dragBox.y + 24)
  await page.mouse.down()
  await page.mouse.move(dragBox.x + 12 + dragZoom, dragBox.y + 24, { steps: 5 })
  await expect(page.locator('.composition-move-ghost')).toHaveCount(2)
  await expect(page.locator('.composition-move-ghost.invalid')).toHaveCount(0)
  assert.deepEqual((await tool('get_project')).data.document, dragBefore)
  assert.equal((await tool('get_history')).data.undoCount, dragHistory)
  const ghostLefts = await page.locator('.composition-move-ghost').evaluateAll((nodes) => nodes.map((node) => parseFloat(node.style.left)))
  await page.mouse.up()
  await expect(page.locator('.composition-move-ghost')).toHaveCount(0)
  const dragAfter = (await tool('get_project')).data.document
  for (const [index, label] of ['A', 'B'].entries()) {
    const landed = dragAfter.tracks.find((track) => track.id === `link-track-${label}`).clips[0]
    assert.ok(Math.abs(ghostLefts[index] - landed.timelineStartTicks / 1_000_000 * dragZoom) < 0.01, JSON.stringify({ label, ghost: ghostLefts[index], start: landed.timelineStartTicks, dragZoom }))
  }
  assert.equal((await tool('get_history')).data.undoCount, dragHistory + 1)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, dragBefore)
  const verticalAnchor = await dragAnchor.boundingBox()
  const verticalDestination = await page.locator('[data-track-id="link-track-B"] .composition-track-lane').boundingBox()
  assert.ok(verticalAnchor && verticalDestination)
  await page.mouse.move(verticalAnchor.x + 12, verticalAnchor.y + 24)
  await page.mouse.down()
  await page.mouse.move(verticalAnchor.x + 12 + dragZoom, verticalDestination.y + 24, { steps: 5 })
  await expect(page.locator('[data-track-id="link-track-B"] .composition-move-ghost')).toHaveCount(1)
  await expect(page.locator('[data-track-id="link-track-C"] .composition-move-ghost')).toHaveCount(1)
  await expect(page.locator('.composition-move-ghost.invalid')).toHaveCount(0)
  assert.deepEqual((await tool('get_project')).data.document, dragBefore)
  await page.mouse.up()
  const verticalAfter = (await tool('get_project')).data.document
  assert.equal(verticalAfter.tracks.find((track) => track.id === 'link-track-A').clips.length, 0)
  assert.equal(verticalAfter.tracks.find((track) => track.id === 'link-track-B').clips[0].id, 'link-clip-A')
  assert.equal(verticalAfter.tracks.find((track) => track.id === 'link-track-C').clips[0].id, 'link-clip-B')
  assert.equal((await tool('get_history')).data.undoCount, dragHistory + 1)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, dragBefore)
  assert.equal((await tool('get_history')).data.undoCount, toggleHistory + 1)
  await page.keyboard.press('Alt+.')
  let unlinkedMoved = await tool('get_project')
  const movedA = unlinkedMoved.data.document.tracks.find((track) => track.id === 'link-track-A').clips[0].timelineStartTicks
  const movedB = unlinkedMoved.data.document.tracks.find((track) => track.id === 'link-track-B').clips[0].timelineStartTicks
  assert.ok(movedA > 0)
  assert.equal(movedB - movedA, 500_000)
  assert.equal((await tool('undo')).failed, false)
  await page.keyboard.press('Control+d')
  let unlinkedCopies = await tool('get_project')
  assert.equal(unlinkedCopies.data.document.links.length, 0)
  for (const name of ['A', 'B']) assert.equal(unlinkedCopies.data.document.tracks.find((track) => track.id === `link-track-${name}`).clips.length, 2)
  await expect(page.locator('.composition-clip.text.selected')).toHaveCount(2)
  assert.equal((await tool('undo')).failed, false)
  await clipA.click()
  await page.getByRole('button', { name: 'Link B, 2.0s', exact: true }).click({ modifiers: ['Shift'] })
  await expect(page.locator('.composition-clip.text.selected')).toHaveCount(2)
  await page.getByRole('slider', { name: 'Плейхед на дорожке Link A', exact: true }).focus()
  await page.keyboard.press('Control+l')
  linked = await tool('get_project')
  assert.deepEqual(linked.data.document.links[0].memberIds, ['link-clip-A', 'link-clip-B'])
  assert.equal((await tool('get_history')).data.undoCount, toggleHistory + 2)
  const selectedHistory = (await tool('get_history')).data.undoCount
  await clipA.click({ modifiers: ['Shift'] })
  await expect(page.locator('.composition-clip.text.selected')).toHaveCount(0)
  assert.equal((await tool('get_history')).data.undoCount, selectedHistory)
  await clipA.click({ modifiers: ['Alt'] })
  await expect(page.locator('.composition-clip.text.selected')).toHaveCount(1)
  await clipA.focus()
  await page.keyboard.press('Delete')
  let selectionDeleted = await tool('get_project')
  assert.equal(selectionDeleted.data.document.tracks.find((track) => track.id === 'link-track-A').clips.length, 0)
  assert.equal(selectionDeleted.data.document.tracks.find((track) => track.id === 'link-track-B').clips.length, 1)
  assert.equal((await tool('undo')).failed, false)
  await clipA.click()
  await clipA.focus()
  await page.keyboard.press('Delete')
  selectionDeleted = await tool('get_project')
  for (const name of ['A', 'B']) assert.equal(selectionDeleted.data.document.tracks.find((track) => track.id === `link-track-${name}`).clips.length, 0)
  assert.equal((await tool('get_history')).data.undoCount, selectedHistory + 1)
  assert.equal((await tool('undo')).failed, false)
  linked = await tool('get_project')
  const overrideHistory = (await tool('get_history')).data.undoCount
  await page.getByRole('slider', { name: 'Плейхед на дорожке Link A', exact: true }).focus()
  await page.keyboard.press('Alt+Shift+g')
  await expect(page.getByRole('button', { name: 'Править клипы отдельно', exact: true })).toHaveAttribute('aria-pressed', 'true')
  assert.equal((await tool('get_history')).data.undoCount, overrideHistory)
  await page.evaluate(async () => {
    const { selectCompositionClip } = await import('/src/lib/state/composition/core.svelte.ts')
    selectCompositionClip('link-track-A', 'link-clip-A')
  })
  await page.keyboard.press('Alt+.')
  let overridden = await tool('get_project')
  assert.ok(overridden.data.document.tracks.find((track) => track.id === 'link-track-A').clips[0].timelineStartTicks > 0)
  assert.equal(overridden.data.document.tracks.find((track) => track.id === 'link-track-B').clips[0].timelineStartTicks, 500_000)
  assert.equal((await tool('undo')).failed, false)
  await expect(page.getByRole('button', { name: 'Править клипы отдельно', exact: true })).toHaveAttribute('aria-pressed', 'true')
  linked = await tool('get_project')
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'paste_layers', subjects: [{ clipId: 'link-clip-A', id: 'paste-single' }], startTicks: 6_000_000 }] })).failed, false)
  let pasted = await tool('get_project')
  assert.equal(pasted.data.document.links.length, 1)
  assert.equal(pasted.data.document.tracks.find((track) => track.id === 'link-track-B').clips.length, 1)
  assert.equal((await tool('undo')).failed, false)
  linked = await tool('get_project')
  const beforeMoveSet = (await tool('get_history')).data.undoCount
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'move_layers', clipIds: ['link-clip-A', 'link-clip-B'], trackId: 'link-track-A', startTicks: 1_000_000 }] })).failed, false)
  let moveSet = await tool('get_project')
  assert.equal(moveSet.data.document.tracks.find((track) => track.id === 'link-track-A').clips[0].timelineStartTicks, 1_000_000)
  assert.equal(moveSet.data.document.tracks.find((track) => track.id === 'link-track-B').clips[0].timelineStartTicks, 1_500_000)
  assert.equal((await tool('get_history')).data.undoCount, beforeMoveSet + 1)
  assert.equal((await tool('undo')).failed, false)
  linked = await tool('get_project')
  const pasteHistory = (await tool('get_history')).data.undoCount
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'paste_layers', subjects: [{ clipId: 'link-clip-A', id: 'paste-a' }, { clipId: 'link-clip-B', id: 'paste-b' }], startTicks: 6_000_000, linkIds: ['paste-link'] }] })).failed, false)
  pasted = await tool('get_project')
  assert.deepEqual(pasted.data.document.links[1], { id: 'paste-link', memberIds: ['paste-a', 'paste-b'] })
  assert.equal(pasted.data.document.tracks.find((track) => track.id === 'link-track-B').clips[1].timelineStartTicks, 6_500_000)
  assert.equal((await tool('get_history')).data.undoCount, pasteHistory + 1)
  assert.equal((await tool('undo')).failed, false)
  linked = await tool('get_project')
  const beforeCopyCount = (await tool('get_history')).data.undoCount
  await page.getByRole('button', { name: 'Копировать связь после исходных', exact: true }).click()
  let copies = await tool('get_project')
  assert.equal(copies.data.document.links.length, 2)
  assert.equal(copies.data.document.tracks.find((track) => track.id === 'link-track-A').clips[1].timelineStartTicks, 2_500_000)
  assert.equal(copies.data.document.tracks.find((track) => track.id === 'link-track-B').clips[1].timelineStartTicks, 3_000_000)
  assert.equal((await tool('get_history')).data.undoCount, beforeCopyCount + 1)
  assert.equal((await tool('undo')).failed, false)
  linked = await tool('get_project')
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'duplicate_layer', clipId: 'link-clip-A', id: 'mcp-copy', startTicks: 6_000_000 }] })).failed, false)
  copies = await tool('get_project')
  assert.equal(copies.data.document.links.length, 2)
  assert.equal(copies.data.document.tracks.find((track) => track.id === 'link-track-B').clips[1].timelineStartTicks, 6_500_000)
  assert.equal((await tool('undo')).failed, false)
  linked = await tool('get_project')
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'move_layer', clipId: 'link-clip-A', trackId: 'link-track-A', startTicks: 1_000_000 }] })).failed, false)
  linked = await tool('get_project')
  assert.equal(linked.data.document.tracks.find((track) => track.id === 'link-track-B').clips[0].timelineStartTicks, 1_500_000)
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'move_layer', clipId: 'link-clip-B', trackId: 'link-track-B', startTicks: 1_000_000, escapeLink: true }] })).failed, false)
  linked = await tool('get_project')
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'trim_layer', clipId: 'link-clip-A', startTicks: 1_250_000, endTicks: 2_750_000 }] })).failed, false)
  linked = await tool('get_project')
  for (const name of ['A', 'B']) {
    const clip = linked.data.document.tracks.find((track) => track.id === `link-track-${name}`).clips[0]
    assert.equal(clip.timelineStartTicks, 1_250_000)
    assert.equal(clip.durationTicks, 1_500_000)
  }
  assert.equal((await tool('undo')).failed, false)
  assert.equal((await tool('undo')).failed, false)
  linked = await tool('get_project')
  assert.equal((await tool('apply_batch', { expectedRevision: linked.data.revision, operations: [{ op: 'split_layer', clipId: 'link-clip-A', atTicks: 1_750_000, rightClipId: 'link-right' }] })).failed, false)
  assert.equal((await tool('get_project')).data.document.links[0].memberIds.length, 4)
  assert.equal((await tool('undo')).failed, false)
  await page.getByRole('slider', { name: 'Плейхед на дорожке Link A', exact: true }).focus()
  await page.keyboard.press('Alt+Shift+g')
  await expect(page.getByRole('button', { name: 'Править клипы отдельно', exact: true })).toHaveAttribute('aria-pressed', 'false')
  await page.getByRole('button', { name: 'Разорвать связь', exact: true }).click()
  assert.equal((await tool('get_project')).data.document.links.length, 0)
  await page.evaluate(async () => {
    const api = await import('/src/lib/state/composition.svelte.ts')
    for (const [id, local] of [['link-clip-A', 250_000], ['link-clip-B', 750_000]]) {
      const clip = api.compositionState.document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === id)
      api.setCompositionAutomationKeyframe({ kind: 'visual', clipId: id }, 'opacity', 0.8, clip.timelineStartTicks + local)
    }
  })
  const keyA = page.locator('.composition-keyframe-diamond[aria-label^="Ключ link-clip-A opacity "]')
  const keyB = page.locator('.composition-keyframe-diamond[aria-label^="Ключ link-clip-B opacity "]')
  const keyDocument = (await tool('get_project')).data.document
  const keyHistory = (await tool('get_history')).data.undoCount
  const keyZoom = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.zoomPxPerSecond)
  await keyB.scrollIntoViewIfNeeded()
  const keyBoxA = await keyA.boundingBox()
  const keyBoxB = await keyB.boundingBox()
  const keyLaneA = await page.locator('[data-keyframe-track="link-track-A"] .composition-keyframe-lane').boundingBox()
  assert.ok(keyBoxA && keyBoxB && keyLaneA)
  const keyPrimary = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedClipId)
  for (const cancelBox of [true, false]) {
    await page.evaluate(async () => { (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes = [] })
    if (cancelBox) await keyA.click()
    const pressX = Math.max(keyBoxA.x + keyBoxA.width, keyBoxB.x + keyBoxB.width) + 30
    const pressY = keyLaneA.y + keyLaneA.height - 2
    const endX = Math.min(keyBoxA.x, keyBoxB.x) - 12
    const endY = keyBoxB.y + keyBoxB.height / 2
    await page.mouse.move(pressX, pressY)
    await page.mouse.down()
    await page.mouse.move(endX, endY, { steps: 5 })
    await expect(page.locator('.composition-marquee')).toHaveCount(1)
    assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.length), 2)
    assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedClipId), keyPrimary)
    if (cancelBox) {
      await page.mouse.move((keyBoxA.x + keyBoxB.x) / 2, endY)
      assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.length), 1)
      await page.keyboard.press('Escape')
      assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.length), 1)
      assert.deepEqual(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.map((subject) => subject.target.clipId)), ['link-clip-A'])
      await expect(page.locator('.composition-marquee')).toHaveCount(0)
    }
    await page.mouse.up()
    assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.length), cancelBox ? 1 : 2)
    if (cancelBox) assert.deepEqual(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.map((subject) => subject.target.clipId)), ['link-clip-A'])
    await expect(page.locator('.composition-marquee')).toHaveCount(0)
    assert.deepEqual((await tool('get_project')).data.document, keyDocument)
    assert.equal((await tool('get_history')).data.undoCount, keyHistory)
  }
  for (const mode of ['cancel', 'move', 'scale']) {
    await page.evaluate(async () => { (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes = [] })
    await keyA.click()
    await keyB.click({ modifiers: ['Shift'] })
    assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.length), 2)
    const anchor = mode === 'scale' ? keyB : keyA
    await anchor.scrollIntoViewIfNeeded()
    const box = await anchor.boundingBox()
    assert.ok(box)
    if (mode === 'scale') await page.keyboard.down('Alt')
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    await page.mouse.down()
    await page.mouse.move(box.x + box.width / 2 + keyZoom * (mode === 'scale' ? 0.5 : 0.1), box.y + box.height / 2, { steps: 5 })
    await expect.poll(() => page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews.length)).toBe(2)
    assert.deepEqual((await tool('get_project')).data.document, keyDocument)
    assert.equal((await tool('get_history')).data.undoCount, keyHistory)
    const projectedKeys = [await keyA.evaluate((el) => parseFloat(el.style.left)), await keyB.evaluate((el) => parseFloat(el.style.left))]
    if (mode === 'cancel') await page.keyboard.press('Escape')
    await page.mouse.up()
    if (mode === 'scale') await page.keyboard.up('Alt')
    await expect.poll(() => page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews.length)).toBe(0)
    if (mode !== 'cancel') {
      assert.deepEqual([await keyA.evaluate((el) => parseFloat(el.style.left)), await keyB.evaluate((el) => parseFloat(el.style.left))], projectedKeys)
      assert.equal((await tool('get_history')).data.undoCount, keyHistory + 1)
      assert.equal((await tool('undo')).failed, false)
    }
    assert.deepEqual((await tool('get_project')).data.document, keyDocument)
  }
  await page.evaluate(async () => {
    const api = await import('/src/lib/state/composition.svelte.ts')
    const clip = api.compositionState.document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
    api.setCompositionAutomationKeyframe({ kind: 'visual', clipId: clip.id }, 'opacity', 0.2, clip.timelineStartTicks + 1_500_000)
  })
  const curveRow = page.locator('[data-keyframe-track="link-track-A"]')
  await curveRow.locator('.composition-track-head button').click()
  await expect(curveRow.locator('svg.composition-keyframe-curve')).toHaveCount(1)
  await curveRow.scrollIntoViewIfNeeded()
  const highKey = curveRow.locator('.composition-keyframe-diamond[data-keyframe-tick="250000"]')
  const lowKey = curveRow.locator('.composition-keyframe-diamond[data-keyframe-tick="1500000"]')
  assert.ok(Math.abs(await highKey.evaluate((el) => parseFloat(el.style.top)) - 10) < 0.001)
  assert.ok(Math.abs(await lowKey.evaluate((el) => parseFloat(el.style.top)) - 110) < 0.001)
  const curveDocument = (await tool('get_project')).data.document
  const curveHistory = (await tool('get_history')).data.undoCount
  const plotBox = await curveRow.locator('.composition-keyframe-lane').boundingBox()
  const lowBox = await lowKey.boundingBox()
  const highBox = await highKey.boundingBox()
  assert.ok(plotBox && lowBox && highBox)
  await page.mouse.move(lowBox.x + lowBox.width + 30, plotBox.y + 45)
  await page.mouse.down()
  await page.mouse.move(highBox.x - 12, plotBox.y + 2, { steps: 5 })
  const curveSelection = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes)
  assert.deepEqual(curveSelection.map((subject) => [subject.target.clipId, subject.ticks]), [['link-clip-A', [250_000]]])
  await page.mouse.up()
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  assert.equal((await tool('get_history')).data.undoCount, curveHistory)
  await curveRow.screenshot({ path: '/tmp/video-kadr-keyframe-curve-qa.png' })
  await page.evaluate(async () => {
    const api = await import('/src/lib/state/composition.svelte.ts')
    const clip = api.compositionState.document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
    api.setCompositionPlayhead(clip.timelineStartTicks + 875_000)
  })
  const curveLine = curveRow.locator('svg.composition-keyframe-curve polyline:not(.composition-keyframe-tail):not(.composition-keyframe-segment-hit)')
  const hoverCurve = await curveLine.getAttribute('points')
  const pictureLayers = page.locator('.composition-stage .composition-visual-media:not(.composition-canvas-blur), .composition-stage .composition-text-layer')
  const pictureOpacities = () => pictureLayers.evaluateAll((layers) => layers.map((layer) => Number(layer.style.opacity)))
  await expect.poll(pictureOpacities).toContain(0.5)
  const segmentHit = curveRow.locator('.composition-keyframe-segment-hit').first()
  const segmentPoints = (await segmentHit.getAttribute('points')).split(' ')
  const [segmentX, segmentY] = segmentPoints[Math.floor(segmentPoints.length / 2)].split(',').map(Number)
  const segmentPlot = await curveRow.locator('.composition-keyframe-lane').boundingBox()
  const segmentPlayhead = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.transport.playheadTicks)
  await page.mouse.click(segmentPlot.x + segmentX, segmentPlot.y + segmentY, { button: 'right' })
  await expect(page.getByRole('menu', { name: 'Сглаживание выбранных ключей' })).toBeVisible()
  assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.transport.playheadTicks), segmentPlayhead)
  await page.keyboard.press('Escape')
  await highKey.focus()
  await page.keyboard.press('Shift+F10')
  const keyboardMenu = page.getByRole('menu', { name: 'Сглаживание выбранных ключей' })
  await expect(keyboardMenu).toBeVisible()
  await expect(keyboardMenu.getByRole('menuitemradio', { name: 'hold', exact: true })).toBeFocused()
  await page.keyboard.press('ArrowUp')
  await expect(keyboardMenu.getByRole('menuitemradio', { name: 'Elastic InOut', exact: true })).toBeFocused()
  await page.keyboard.press('Home')
  await expect(keyboardMenu.getByRole('menuitemradio', { name: 'hold', exact: true })).toBeFocused()
  await page.keyboard.press('End')
  await page.keyboard.press('Enter')
  await expect(keyboardMenu).toBeHidden()
  await expect(highKey).toBeFocused()
  const keyboardDocument = (await tool('get_project')).data.document
  assert.deepEqual(keyboardDocument.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A').animation.opacity.track.keyframes[0].interpolation, { kind: 'elastic', dir: 'in_out', amplitude: 1, period: 0.3 })
  assert.equal((await tool('get_history')).data.undoCount, curveHistory + 1)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  await highKey.focus()
  await page.keyboard.press('Shift+F10')
  await expect(keyboardMenu).toBeVisible()
  await page.keyboard.press('Tab')
  await expect(keyboardMenu).toBeHidden()
  assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews.length), 0)

  const easingPlayhead = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.transport.playheadTicks)
  await highKey.click({ button: 'right' })
  const easingMenu = page.getByRole('menu', { name: 'Сглаживание выбранных ключей' })
  await expect(easingMenu).toBeVisible()
  await expect(easingMenu.getByRole('menuitemradio', { name: 'linear', exact: true })).toHaveAttribute('aria-checked', 'true')
  await easingMenu.getByRole('menuitemradio', { name: 'ease_in', exact: true }).hover()
  await expect.poll(() => curveLine.getAttribute('points')).not.toBe(hoverCurve)
  await expect.poll(async () => (await pictureOpacities()).some((value) => Math.abs(value - 0.725) < 0.00001)).toBe(true)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  assert.equal((await tool('get_history')).data.undoCount, curveHistory)
  await page.keyboard.press('Escape')
  await expect(easingMenu).toBeHidden()
  await expect.poll(() => curveLine.getAttribute('points')).toBe(hoverCurve)
  await expect.poll(pictureOpacities).toContain(0.5)
  await highKey.click({ button: 'right' })
  await easingMenu.getByRole('menuitemradio', { name: 'ease_in', exact: true }).click()
  await expect(easingMenu).toBeHidden()
  const easedDocument = (await tool('get_project')).data.document
  const easedClip = easedDocument.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
  assert.equal(easedClip.animation.opacity.track.keyframes[0].interpolation, 'ease_in')
  assert.equal(easedClip.animation.opacity.track.keyframes[1].interpolation, undefined)
  assert.equal((await tool('get_history')).data.undoCount, curveHistory + 1)
  assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.transport.playheadTicks), easingPlayhead)
  await highKey.click({ button: 'right' })
  await expect(easingMenu.getByRole('menuitemradio', { name: 'ease_in', exact: true })).toHaveAttribute('aria-checked', 'true')
  await page.keyboard.press('Escape')
  await expect(easingMenu).toBeHidden()
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  for (const bounce of ['bounce_in', 'bounce_out', 'bounce_in_out']) {
    await highKey.click({ button: 'right' })
    await easingMenu.getByRole('menuitemradio', { name: bounce, exact: true }).click()
    const bounceDoc = (await tool('get_project')).data.document
    assert.equal(bounceDoc.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A').animation.opacity.track.keyframes[0].interpolation, bounce)
    assert.equal((await tool('get_history')).data.undoCount, curveHistory + 1)
    assert.equal((await tool('undo')).failed, false)
    assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  }
  for (const [name, dir] of [['Elastic In', 'in'], ['Elastic Out', 'out'], ['Elastic InOut', 'in_out']]) {
    await highKey.click({ button: 'right' })
    await easingMenu.getByRole('menuitemradio', { name, exact: true }).click()
    const elasticDoc = (await tool('get_project')).data.document
    assert.deepEqual(elasticDoc.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A').animation.opacity.track.keyframes[0].interpolation, { kind: 'elastic', dir, amplitude: 1, period: 0.3 })
    assert.equal((await tool('get_history')).data.undoCount, curveHistory + 1)
    assert.equal((await tool('undo')).failed, false)
    assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  }
  await highKey.click({ button: 'right' })
  await easingMenu.getByRole('menuitemradio', { name: 'Elastic Out', exact: true }).click()
  const authoredElastic = (await tool('get_project')).data.document
  await highKey.click({ button: 'right' })
  const selectedAmplitude = easingMenu.getByRole('spinbutton', { name: 'Амплитуда Elastic выбранных ключей', exact: true })
  await selectedAmplitude.fill('2')
  assert.deepEqual((await tool('get_project')).data.document, authoredElastic)
  await selectedAmplitude.press('Escape')
  assert.deepEqual((await tool('get_project')).data.document, authoredElastic)
  await highKey.click({ button: 'right' })
  await selectedAmplitude.fill('2')
  await selectedAmplitude.press('Tab')
  const selectedPeriod = easingMenu.getByRole('spinbutton', { name: 'Период Elastic выбранных ключей', exact: true })
  await selectedPeriod.fill('0.17')
  await selectedPeriod.press('Tab')
  assert.deepEqual((await tool('get_project')).data.document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A').animation.opacity.track.keyframes[0].interpolation, { kind: 'elastic', dir: 'out', amplitude: 2, period: 0.17 })
  assert.equal((await tool('get_history')).data.undoCount, curveHistory + 3)
  await page.keyboard.press('Escape')
  for (let count = 0; count < 3; count++) assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  await highKey.click({ button: 'right' })
  await easingMenu.getByRole('menuitemradio', { name: 'Bézier', exact: true }).hover()
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  await page.keyboard.press('Escape')
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  await highKey.click({ button: 'right' })
  await easingMenu.getByRole('menuitemradio', { name: 'Bézier', exact: true }).click()
  const authoredBezier = (await tool('get_project')).data.document
  assert.deepEqual(authoredBezier.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A').animation.opacity.track.keyframes[0].interpolation, { kind: 'bezier', p1: [0.25, 0.1], p2: [0.25, 1] })
  await highKey.click({ button: 'right' })
  const selectedBezierY = easingMenu.getByRole('spinbutton', { name: 'Bézier p1 Y выбранных ключей', exact: true })
  await selectedBezierY.fill('-2')
  assert.deepEqual((await tool('get_project')).data.document, authoredBezier)
  await selectedBezierY.press('Escape')
  assert.deepEqual((await tool('get_project')).data.document, authoredBezier)
  await highKey.click({ button: 'right' })
  await selectedBezierY.fill('0.2')
  await selectedBezierY.press('Enter')
  await selectedBezierY.press('Tab')
  assert.deepEqual((await tool('get_project')).data.document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A').animation.opacity.track.keyframes[0].interpolation, { kind: 'bezier', p1: [0.25, 0.2], p2: [0.25, 1] })
  assert.equal((await tool('get_history')).data.undoCount, curveHistory + 2)
  await easingMenu.screenshot({ path: '/tmp/video-kadr-bezier-menu-qa.png' })
  await page.keyboard.press('Escape')
  for (let count = 0; count < 2; count++) assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  await lowKey.click({ button: 'right' })
  await expect(easingMenu).toBeVisible()
  const seekClip = curveDocument.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
  assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.transport.playheadTicks), seekClip.timelineStartTicks + 1_500_000)
  const seekSelection = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes)
  assert.deepEqual(seekSelection.map((subject) => [subject.target.clipId, subject.ticks]), [['link-clip-A', [1_500_000]]])
  await page.keyboard.press('Escape')
  await expect(easingMenu).toBeHidden()
  assert.equal((await tool('get_history')).data.undoCount, curveHistory)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)


  await lowKey.focus()
  await page.keyboard.press('Control+c')
  assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.clipboard.kind), 'keyframes')
  const clipboardClip = page.locator('[data-track-id="link-track-A"] .composition-clip.text').first()
  await clipboardClip.click()
  await page.evaluate(async () => {
    const api = await import('/src/lib/state/composition.svelte.ts')
    const clip = api.compositionState.document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
    api.setCompositionPlayhead(clip.timelineStartTicks + 1_000_000)
  })
  await clipboardClip.focus()
  await page.keyboard.press('Control+v')
  await expect.poll(async () => (await tool('get_history')).data.undoCount).toBe(curveHistory + 1)
  const keyPastedDocument = (await tool('get_project')).data.document
  const keyPastedClip = keyPastedDocument.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
  assert.deepEqual(keyPastedClip.animation.opacity.track.keyframes.map((key) => key.tick), [250_000, 1_000_000, 1_500_000])
  assert.equal(keyPastedClip.animation.opacity.track.keyframes[1].value, 0.2)
  assert.deepEqual(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedKeyframes.map((subject) => subject.ticks)), [[1_000_000]])
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)
  await clipboardClip.click()
  await page.keyboard.press('Control+c')
  assert.equal(await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.clipboard.kind), 'layer')
  await page.evaluate(async () => {
    const api = await import('/src/lib/state/composition.svelte.ts')
    const clip = api.compositionState.document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
    api.setCompositionPlayhead(clip.timelineStartTicks)
  })
  await clipboardClip.focus()
  await page.keyboard.press('Control+v')
  await expect.poll(async () => (await tool('get_history')).data.undoCount).toBe(curveHistory + 1)
  const clipPastedDocument = (await tool('get_project')).data.document
  const cloneId = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.selectedClipId)
  assert.notEqual(cloneId, 'link-clip-A')
  assert.equal(clipPastedDocument.tracks.reduce((sum, track) => sum + track.clips.length, 0), curveDocument.tracks.reduce((sum, track) => sum + track.clips.length, 0) + 1)
  assert.deepEqual(clipPastedDocument.tracks.flatMap((track) => track.clips).find((clip) => clip.id === cloneId).animation, seekClip.animation)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, curveDocument)

  // Seed authored tracks; use the actual inspector and pointer surface for Scale edits.
  await clipboardClip.click()
  await page.evaluate(async () => {
    const api = await import('/src/lib/state/composition.svelte.ts')
    api.applyCompositionVisualAnimation('link-clip-A', { scaleX: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 250_000, value: 1, interpolation: 'ease_in' }, { tick: 1_500_000, value: 2 }] } }, scaleY: { mode: 'constant', value: 3 } })
  })
  const beforeScaleLink = (await tool('get_project')).data.document
  const scaleLinkDepth = (await tool('get_history')).data.undoCount
  const scaleLink = page.getByRole('checkbox', { name: 'Связать масштаб X и Y', exact: true })
  await scaleLink.check()
  const scaleOf = (document) => document.tracks.flatMap((track) => track.clips).find((clip) => clip.id === 'link-clip-A')
  let scaleDocument = (await tool('get_project')).data.document
  assert.equal(scaleOf(scaleDocument).scaleLinked, true)
  assert.deepEqual(scaleOf(scaleDocument).animation.scaleX, scaleOf(scaleDocument).animation.scaleY)
  assert.equal((await tool('get_history')).data.undoCount, scaleLinkDepth + 1)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, beforeScaleLink)
  await scaleLink.check()
  scaleDocument = (await tool('get_project')).data.document
  const scaleDepth = (await tool('get_history')).data.undoCount
  const scaleParameter = page.getByRole('combobox', { name: 'Параметр keyframes', exact: true })
  await scaleParameter.selectOption('scaleX')
  await expect(scaleParameter.locator('option[value="scaleY"]')).toHaveCount(0)
  await expect(scaleParameter.locator('option[value="scaleX"]')).toHaveText('Scale')
  const scaleValue = page.getByRole('spinbutton', { name: 'Значение keyframe Scale X', exact: true }).first()
  await scaleValue.fill('1.5')
  await scaleValue.press('Tab')
  const valueDocument = (await tool('get_project')).data.document
  assert.equal(scaleOf(valueDocument).animation.scaleX.track.keyframes[0].value, 1.5)
  assert.deepEqual(scaleOf(valueDocument).animation.scaleX, scaleOf(valueDocument).animation.scaleY)
  assert.equal(scaleOf(valueDocument).scaleLinked, true)
  assert.equal((await tool('get_history')).data.undoCount, scaleDepth + 1)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, scaleDocument)
  await expect(page.locator('[aria-label^="Ключ link-clip-A scaleY"]')).toHaveCount(0)
  const scaleKey = page.locator('[aria-label="Ключ link-clip-A scaleX 250000"]')
  for (const cancel of [true, false]) {
    await scaleKey.scrollIntoViewIfNeeded()
    await scaleKey.click()
    const bounds = await scaleKey.boundingBox()
    assert.ok(bounds)
    const zoom = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.zoomPxPerSecond)
    const x = bounds.x + bounds.width / 2
    const y = bounds.y + bounds.height / 2
    await page.mouse.move(x, y)
    await page.mouse.down()
    await page.mouse.move(x + zoom * 0.1, y, { steps: 5 })
    const previews = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews)
    assert.deepEqual(previews.map((entry) => entry.property).sort(), ['scaleX', 'scaleY'])
    assert.deepEqual(previews[0].value, previews[1].value)
    assert.deepEqual((await tool('get_project')).data.document, scaleDocument)
    assert.equal((await tool('get_history')).data.undoCount, scaleDepth)
    if (cancel) await page.keyboard.press('Escape')
    await page.mouse.up()
    await expect.poll(async () => page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews.length)).toBe(0)
    if (!cancel) {
      const landed = (await tool('get_project')).data.document
      assert.deepEqual(scaleOf(landed).animation.scaleX, previews[0].value)
      assert.deepEqual(scaleOf(landed).animation.scaleX, scaleOf(landed).animation.scaleY)
      assert.equal(scaleOf(landed).scaleLinked, true)
      assert.equal((await tool('get_history')).data.undoCount, scaleDepth + 1)
      assert.equal((await tool('undo')).failed, false)
    }
    assert.deepEqual((await tool('get_project')).data.document, scaleDocument)
  }

  // Exercise timing handles with real pointer capture and the linked preview bus.
  await scaleParameter.selectOption('scaleX')
  const timingPicker = page.getByRole('combobox', { name: 'Интерполяция keyframes', exact: true })
  await timingPicker.selectOption('bezier')
  const timingDocument = (await tool('get_project')).data.document
  const timingDepth = (await tool('get_history')).data.undoCount
  const timingGraph = page.getByRole('group', { name: 'Контрольные точки Bézier', exact: true })
  const timingHandle = timingGraph.getByRole('button', { name: 'Переместить Bézier p1', exact: true })
  for (const cancel of [true, false]) {
    await timingHandle.scrollIntoViewIfNeeded()
    const bounds = await timingHandle.boundingBox()
    assert.ok(bounds)
    const x = bounds.x + bounds.width / 2
    const y = bounds.y + bounds.height / 2
    await page.mouse.move(x, y)
    await page.mouse.down()
    await page.mouse.move(x + 15, y - 10, { steps: 5 })
    const previews = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews)
    assert.deepEqual(previews.map(entry => entry.property).sort(), ['scaleX', 'scaleY'])
    assert.deepEqual(previews[0].value, previews[1].value)
    assert.deepEqual((await tool('get_project')).data.document, timingDocument)
    assert.equal((await tool('get_history')).data.undoCount, timingDepth)
    if (cancel) await page.keyboard.press('Escape')
    await page.mouse.up()
    await expect.poll(async () => page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews.length)).toBe(0)
    if (!cancel) {
      const landed = (await tool('get_project')).data.document
      assert.deepEqual(scaleOf(landed).animation.scaleX, previews[0].value)
      assert.deepEqual(scaleOf(landed).animation.scaleX, scaleOf(landed).animation.scaleY)
      assert.equal((await tool('get_history')).data.undoCount, timingDepth + 1)
      await timingGraph.screenshot({ path: '/tmp/video-kadr-bezier-handles-qa.png' })
      assert.equal((await tool('undo')).failed, false)
    }
    assert.deepEqual((await tool('get_project')).data.document, timingDocument)
  }
  await scaleKey.click({ button: 'right' })
  await page.getByRole('menu', { name: 'Сглаживание выбранных ключей', exact: true }).getByRole('menuitemradio', { name: 'Bézier', exact: true }).click()
  const tangentDocument = (await tool('get_project')).data.document
  const tangentDepth = (await tool('get_history')).data.undoCount
  const tangentRow = scaleKey.locator('xpath=ancestor::div[contains(@class,"composition-keyframe-row")]')
  const tangentExpand = tangentRow.locator('.composition-track-head button')
  if (await tangentExpand.getAttribute('aria-expanded') !== 'true') await tangentExpand.click()
  const tangentHandle = tangentRow.getByRole('button', { name: 'Касательная scaleX outgoing 250000', exact: true })
  for (const cancel of [true, false]) {
    await tangentHandle.scrollIntoViewIfNeeded()
    const bounds = await tangentHandle.boundingBox()
    assert.ok(bounds)
    const x = bounds.x + bounds.width/2, y = bounds.y + bounds.height/2
    await page.mouse.move(x,y)
    await page.mouse.down()
    await page.mouse.move(x+10,y-10,{ steps: 5 })
    const previews = await page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews)
    assert.deepEqual(previews.map(entry => entry.property).sort(), ['scaleX','scaleY'])
    assert.deepEqual(previews[0].value, previews[1].value)
    assert.deepEqual((await tool('get_project')).data.document, tangentDocument)
    assert.equal((await tool('get_history')).data.undoCount,tangentDepth)
    if (cancel) await page.keyboard.press('Escape')
    await page.mouse.up()
    await expect.poll(async () => page.evaluate(async () => (await import('/src/lib/state/composition.svelte.ts')).compositionState.ui.keyframePreviews.length)).toBe(0)
    if (!cancel) {
      const changed = (await tool('get_project')).data.document
      assert.deepEqual(scaleOf(changed).animation.scaleX,previews[0].value)
      assert.deepEqual(scaleOf(changed).animation.scaleX,scaleOf(changed).animation.scaleY)
      assert.equal((await tool('get_history')).data.undoCount,tangentDepth+1)
      await tangentRow.screenshot({ path: '/tmp/video-kadr-timeline-tangents-qa.png' })
      assert.equal((await tool('undo')).failed,false)
    }
    assert.deepEqual((await tool('get_project')).data.document,tangentDocument)
  }
  await tangentHandle.focus()
  await tangentHandle.press('Shift+F10')
  const continuityMenu = page.getByRole('menu',{ name: 'Непрерывность касательной',exact: true })
  await expect(continuityMenu).toBeVisible()
  await page.keyboard.press('End')
  await page.keyboard.press('ArrowDown')
  await page.keyboard.press('Enter')
  const continuityDocument = (await tool('get_project')).data.document
  assert.equal(scaleOf(continuityDocument).animation.scaleX.track.keyframes[0].tangents.continuity,'smooth')
  assert.deepEqual(scaleOf(continuityDocument).animation.scaleX,scaleOf(continuityDocument).animation.scaleY)
  assert.equal((await tool('get_history')).data.undoCount,tangentDepth+1)
  assert.equal((await tool('undo')).failed,false)
  assert.deepEqual((await tool('get_project')).data.document,tangentDocument)
  assert.equal((await tool('undo')).failed,false)
  assert.deepEqual((await tool('get_project')).data.document,timingDocument)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, scaleDocument)

  // Named presets traverse the real MCP bridge and preserve linked scale/history.
  const namedPresets = await page.evaluate(async () => (await import('/src/lib/composition/easingPresets.ts')).BEZIER_PRESETS)
  assert.equal(namedPresets.length,28)
  for (const preset of namedPresets) {
    const snapshot = (await tool('get_project')).data
    const depth = (await tool('get_history')).data.undoCount
    const request = { expectedRevision: snapshot.revision,operations: [{ op: 'set_keyframe_preset',clipId: 'link-clip-A',property: 'scaleX',ticks: [250_000],preset: preset.id }] }
    assert.equal((await tool('dry_run',request)).failed,false)
    assert.deepEqual((await tool('get_project')).data.document,snapshot.document)
    assert.equal((await tool('get_history')).data.undoCount,depth)
    assert.equal((await tool('apply_batch',request)).failed,false)
    const changed = (await tool('get_project')).data.document
    assert.deepEqual(scaleOf(changed).animation.scaleX,scaleOf(changed).animation.scaleY)
    assert.deepEqual(scaleOf(changed).animation.scaleX.track.keyframes[0].interpolation,preset.interpolation)
    assert.equal((await tool('get_history')).data.undoCount,depth+1)
    assert.equal((await tool('undo')).failed,false)
    assert.deepEqual((await tool('get_project')).data.document,snapshot.document)
  }

  // Physical repetition controls must fan out to linked Scale and undo exactly.
  await scaleParameter.selectOption('scaleX')
  const afterKeys = page.getByRole('combobox', { name: 'Продолжение после ключей', exact: true })
  const beforeKeys = page.getByRole('combobox', { name: 'Продолжение до ключей', exact: true })
  for (const mode of ['continue', 'loop', 'ping_pong', 'offset']) {
    const original = (await tool('get_project')).data.document
    const depth = (await tool('get_history')).data.undoCount
    await afterKeys.selectOption(mode)
    const changed = (await tool('get_project')).data.document
    assert.deepEqual(scaleOf(changed).animation.scaleX.track.extrapolation, { before: 'hold', after: mode })
    assert.deepEqual(scaleOf(changed).animation.scaleX, scaleOf(changed).animation.scaleY)
    assert.equal(scaleOf(changed).scaleLinked, true)
    assert.equal((await tool('get_history')).data.undoCount, depth + 1)
    await beforeKeys.selectOption(mode)
    const both = (await tool('get_project')).data.document
    assert.deepEqual(scaleOf(both).animation.scaleX.track.extrapolation, { before: mode, after: mode })
    assert.deepEqual(scaleOf(both).animation.scaleX, scaleOf(both).animation.scaleY)
    assert.equal((await tool('get_history')).data.undoCount, depth + 2)
    const row = scaleKey.locator('xpath=ancestor::div[contains(@class,"composition-keyframe-row")]')
    const expand = row.locator('.composition-track-head button')
    if (await expand.getAttribute('aria-expanded') !== 'true') await expand.click()
    const curve = row.locator('svg.composition-keyframe-curve[data-keyframe-curve*="scaleX"]')
    await expect(curve).toBeVisible()
    const points = await curve.locator('.composition-keyframe-tail').first().getAttribute('points')
    assert.ok(points && !points.includes('NaN'))
    if (mode !== 'continue') assert.ok(points.split(' ').length > 2)
    await row.screenshot({ path: `/tmp/video-kadr-extrapolation-${mode}.png` })
    assert.equal((await tool('undo')).failed, false)
    assert.deepEqual((await tool('get_project')).data.document, changed)
    assert.equal((await tool('undo')).failed, false)
    assert.deepEqual((await tool('get_project')).data.document, original)
  }

  // Real session commands must preserve the new authored-key trim/split contracts.
  for (const operation of ['trim', 'split']) {
    const snapshot = await tool('get_project')
    const original = snapshot.data.document
    const originalClip = scaleOf(original)
    const depth = (await tool('get_history')).data.undoCount
    const request = operation === 'trim'
      ? { op: 'trim_layer', clipId: originalClip.id, startTicks: originalClip.timelineStartTicks + 250_000, endTicks: originalClip.timelineStartTicks + originalClip.durationTicks - 250_000 }
      : { op: 'split_layer', clipId: originalClip.id, atTicks: originalClip.timelineStartTicks + 750_000, rightClipId: 'scale-split-right' }
    assert.equal((await tool('apply_batch', { expectedRevision: snapshot.data.revision, operations: [request] })).failed, false)
    const changed = (await tool('get_project')).data.document
    const left = scaleOf(changed)
    const track = originalClip.animation.scaleX.track
    const expectedLeft = operation === 'trim' ? track.keyframes.map(key => ({ ...key, tick: key.tick - 250_000 })) : track.keyframes.filter(key => key.tick <= 750_000)
    assert.deepEqual(left.animation.scaleX.track.keyframes, expectedLeft)
    assert.deepEqual(left.animation.scaleX, left.animation.scaleY)
    if (operation === 'split') {
      const right = changed.tracks.flatMap(track => track.clips).find(clip => clip.id === 'scale-split-right')
      assert.ok(right)
      assert.deepEqual(right.animation.scaleX.track.keyframes, track.keyframes.filter(key => key.tick > 750_000).map(key => ({ ...key, tick: key.tick - 750_000 })))
      assert.deepEqual(right.animation.scaleX, right.animation.scaleY)
    }
    assert.equal((await tool('get_history')).data.undoCount, depth + 1)
    assert.equal((await tool('undo')).failed, false)
    assert.deepEqual((await tool('get_project')).data.document, original)
  }

  const roleGain = page.getByRole('spinbutton', { name: 'Громкость роли music', exact: true })
  const roleMute = page.getByRole('checkbox', { name: 'Mute роли music', exact: true })
  const roleSolo = page.getByRole('checkbox', { name: 'Solo роли music', exact: true })
  const roleDepth = (await tool('get_history')).data.undoCount
  await roleGain.fill('-6')
  await roleGain.press('Tab')
  assert.equal((await tool('get_history')).data.undoCount, roleDepth + 1)
  const roleCheckpoint = await tool('create_checkpoint', { label: 'Role gain -6' })
  assert.equal(roleCheckpoint.failed, false)
  await roleGain.fill('-12')
  await roleGain.press('Tab')
  assert.equal((await tool('get_history')).data.undoCount, roleDepth + 2)
  await roleMute.check()
  await roleSolo.check()
  const musicMix = async () => (await tool('get_project')).data.document.audioRoles.music
  assert.deepEqual(await musicMix(), { gainDb: -12, muted: true, solo: true })
  assert.equal((await tool('get_history')).data.undoCount, roleDepth + 2)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual(await musicMix(), { gainDb: -6, muted: true, solo: true })
  await expect(roleGain).toHaveValue('-6')
  await expect(roleMute).toBeChecked()
  await expect(roleSolo).toBeChecked()
  assert.equal((await tool('redo')).failed, false)
  assert.deepEqual(await musicMix(), { gainDb: -12, muted: true, solo: true })
  assert.equal((await tool('restore_checkpoint', { id: roleCheckpoint.data.id })).failed, false)
  assert.deepEqual(await musicMix(), { gainDb: -6, muted: true, solo: true })
  await expect(roleGain).toHaveValue('-6')
  await roleMute.uncheck()
  assert.deepEqual(await musicMix(), { gainDb: -6, muted: false, solo: true })
  const roleFader = page.getByRole('slider', { name: 'Прослушать громкость роли music', exact: true })
  const faderBefore = (await tool('get_project')).data.document
  const faderBounds = await roleFader.boundingBox()
  assert.ok(faderBounds)
  await page.mouse.move(faderBounds.x + faderBounds.width * 0.5, faderBounds.y + faderBounds.height / 2)
  await page.mouse.down()
  await page.mouse.move(faderBounds.x + faderBounds.width * 0.7, faderBounds.y + faderBounds.height / 2)
  assert.deepEqual((await tool('get_project')).data.document, faderBefore)
  const faderDraft = Number(await roleFader.inputValue())
  await expect(roleGain).toHaveValue(String(faderDraft))
  await page.mouse.up()
  assert.equal((await musicMix()).gainDb, faderDraft)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, faderBefore)
  await page.mouse.move(faderBounds.x + faderBounds.width * 0.8, faderBounds.y + faderBounds.height / 2)
  await page.mouse.down()
  await roleFader.press('Escape')
  await page.mouse.up()
  assert.deepEqual((await tool('get_project')).data.document, faderBefore)
  const beforePreviewControls = await tool('get_project')
  const numericDepth = (await tool('get_history')).data.undoCount
  await roleGain.fill('9')
  await roleGain.press('Escape')
  assert.deepEqual((await tool('get_project')).data.document, beforePreviewControls.data.document)
  assert.equal((await tool('get_history')).data.undoCount, numericDepth)
  await expect(roleGain).toHaveValue(String((await musicMix()).gainDb))
  await roleGain.fill('-4.5')
  await roleGain.press('Enter')
  assert.equal((await musicMix()).gainDb, -4.5)
  assert.equal((await tool('get_history')).data.undoCount, numericDepth + 1)
  assert.equal((await tool('undo')).failed, false)
  assert.deepEqual((await tool('get_project')).data.document, beforePreviewControls.data.document)
  const previewMute = page.getByRole('checkbox', { name: 'Отключить звук предпросмотра', exact: true })
  await previewMute.check()
  await expect(previewMute).toBeChecked()
  await page.getByRole('button', { name: 'Сбросить пиковый уровень звука', exact: true }).click()
  await previewMute.uncheck()
  assert.deepEqual((await tool('get_project')).data.document, beforePreviewControls.data.document)
  const mixer = roleGain.locator('xpath=ancestor::fieldset[1]')
  await mixer.screenshot({ path: '/tmp/video-kadr-role-mixer-qa.png' })

  assert.equal((await tool('set_history_lock', { locked: true, reason: 'Disconnect test' })).failed, false)
  await page.getByRole('status').filter({ hasText: 'История занята: Disconnect test' }).waitFor()
  assert.equal((await fetch(`${backendUrl}/mcp`, { method: 'DELETE', headers: { Authorization: `Bearer ${token}`, 'Mcp-Session-Id': sessionId } })).status, 204)
  await page.getByRole('status').filter({ hasText: 'История занята: Disconnect test' }).waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: 'Отключить агента', exact: true }).click()
  console.log('Real MCP → browser editor: batch/history/search/resources/disconnect and real FFmpeg shot scan, measurement, frame comparison, review images, atomic split/Undo, UI link creation/removal and UI/MCP linked duplicate with independent links and Undo, MCP linked move/trim/split with escape, session override keyboard/nudge/Undo/MCP isolation, linked/Alt/Shift selection and exact-selection delete/Undo and Ctrl+L unlink/relink, explicit-subject MCP paste and cross-track Ctrl+D, exact-subject MCP move, unlinked multi-selection nudge, clip marquee across lanes with live shrinking, Escape restoration and ordinary blank-gap click and edge autoscroll cancellation, and physical multi-subject same-lane and relative cross-lane drag with matching ghosts, unchanged document until release and one Undo each, plus physical multi-layer keyframe marquee/shrink/Escape identity restoration after mouseup, and move/Alt-scale/Escape with shared previews and one Undo, expanded value curves with two-axis key marquee and screenshot, physical curve-segment right click and Shift+F10/arrows/Home/End/Enter/Tab context interaction, context EaseIn hover curve/picture preview with Escape restoration, selection/checkmark/Escape, physical global keyframe and clip copy/paste with one Undo each passed.')
} finally {
  await browser?.close()
  await server?.close()
  backend.kill('SIGTERM')
  await new Promise((done) => {
    if (backend.exitCode !== null) return done()
    const timer = setTimeout(() => { backend.kill('SIGKILL'); done() }, 5000)
    backend.once('exit', () => { clearTimeout(timer); done() })
  })
  await rm(storage, { recursive: true, force: true })
}
