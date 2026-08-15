import { spawn, spawnSync } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const repositoryRoot = resolve(frontendRoot, '..')
const backendRoot = join(repositoryRoot, 'backend')
const cargoTargetRoot = process.env.CARGO_TARGET_DIR
  ? resolve(backendRoot, process.env.CARGO_TARGET_DIR)
  : join(backendRoot, 'target')
const backendBinary = join(
  cargoTargetRoot,
  'debug',
  process.platform === 'win32' ? 'video-kadr-backend.exe' : 'video-kadr-backend',
)
const children = new Set()
let temporaryRoot
let cleaningUp = false

function hasCommand(command) {
  return spawnSync(command, ['-version'], { stdio: 'ignore' }).status === 0
}

function reservePort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer()
    server.unref()
    server.once('error', (error) => {
      if (error?.code === 'EPERM' || error?.code === 'EACCES') {
        reject(new Error(
          'Real composition E2E requires permission to bind temporary 127.0.0.1 ports; '
          + 'lack of loopback permission is a test failure, not an auto-skip.',
          { cause: error },
        ))
        return
      }
      reject(error)
    })
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (!address || typeof address === 'string') {
        server.close(() => reject(new Error('Failed to reserve a loopback port')))
        return
      }
      server.close((error) => error ? reject(error) : resolvePort(address.port))
    })
  })
}

function spawnBounded(command, args, options, timeoutMs) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, {
      ...options,
      detached: process.platform !== 'win32',
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    children.add(child)
    let output = ''
    const append = (chunk) => {
      output = (output + chunk.toString()).slice(-24_000)
    }
    child.stdout.on('data', append)
    child.stderr.on('data', append)
    const timer = setTimeout(() => {
      stopChild(child, 'SIGKILL')
      reject(new Error(`${command} timed out after ${timeoutMs}ms\n${output}`))
    }, timeoutMs)
    timer.unref()
    child.once('error', (error) => {
      clearTimeout(timer)
      children.delete(child)
      reject(error)
    })
    child.once('exit', (code, signal) => {
      clearTimeout(timer)
      children.delete(child)
      if (code === 0) resolveRun(output)
      else reject(new Error(`${command} exited with ${code ?? signal}\n${output}`))
    })
  })
}

function spawnServer(command, args, options) {
  const child = spawn(command, args, {
    ...options,
    detached: process.platform !== 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  children.add(child)
  let output = ''
  let launchError
  const append = (chunk) => {
    output = (output + chunk.toString()).slice(-24_000)
  }
  child.stdout.on('data', append)
  child.stderr.on('data', append)
  child.once('error', (error) => {
    launchError = error
    append(`${error.name}: ${error.message}\n`)
    children.delete(child)
  })
  child.once('exit', () => children.delete(child))
  child.getOutput = () => output
  child.getLaunchError = () => launchError
  return child
}

function stopChild(child, signal = 'SIGTERM') {
  if (!child.pid || child.exitCode !== null) return
  try {
    if (process.platform === 'win32') child.kill(signal)
    else process.kill(-child.pid, signal)
  } catch (error) {
    if (error?.code !== 'ESRCH') throw error
  }
}

async function waitForUrl(url, child, label, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs
  let lastError = ''
  while (Date.now() < deadline) {
    if (child.getLaunchError()) {
      throw new Error(`${label} could not start\n${child.getOutput()}`, {
        cause: child.getLaunchError(),
      })
    }
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error(`${label} exited before becoming ready\n${child.getOutput()}`)
    }
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(1_000) })
      if (response.ok) return
      lastError = `HTTP ${response.status}`
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error)
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 150))
  }
  throw new Error(`${label} did not become ready: ${lastError}\n${child.getOutput()}`)
}

async function cleanup() {
  if (cleaningUp) return
  cleaningUp = true
  const running = [...children]
  const errors = []
  for (const child of running) {
    try { stopChild(child) } catch (error) { errors.push(error) }
  }
  await new Promise((resolveWait) => setTimeout(resolveWait, 300))
  for (const child of running) {
    try { stopChild(child, 'SIGKILL') } catch (error) { errors.push(error) }
  }
  if (temporaryRoot) {
    try { await rm(temporaryRoot, { recursive: true, force: true }) } catch (error) { errors.push(error) }
  }
  if (errors.length) throw new AggregateError(errors, 'Real composition E2E cleanup failed')
}

for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143]]) {
  process.once(signal, () => {
    void cleanup().finally(() => process.exit(code))
  })
}

if (!hasCommand('ffmpeg') || !hasCommand('ffprobe')) {
  console.log('SKIP real composition E2E: ffmpeg and ffprobe are required on PATH')
  process.exit(0)
}

try {
  temporaryRoot = await mkdtemp(join(tmpdir(), 'video-kadr-real-e2e-'))
  const storageDir = join(temporaryRoot, 'storage')
  const fixturePath = join(temporaryRoot, 'real-composition-fixture.mp4')
  const [backendPort, frontendPort] = await Promise.all([reservePort(), reservePort()])
  const backendUrl = `http://127.0.0.1:${backendPort}`
  const frontendUrl = `http://127.0.0.1:${frontendPort}`

  await spawnBounded(
    'ffmpeg',
    [
      '-hide_banner', '-loglevel', 'error', '-y',
      '-f', 'lavfi', '-i', 'color=c=#1746a2:s=160x90:r=10:d=1',
      '-f', 'lavfi', '-i', 'sine=frequency=523.25:sample_rate=48000:duration=1',
      '-shortest', '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '96k', '-movflags', '+faststart', fixturePath,
    ],
    { cwd: repositoryRoot, env: process.env },
    30_000,
  )

  await spawnBounded(
    'cargo',
    ['build', '--bin', 'video-kadr-backend'],
    { cwd: backendRoot, env: process.env },
    240_000,
  )

  const backend = spawnServer(backendBinary, [], {
    cwd: backendRoot,
    env: {
      ...process.env,
      STORAGE_DIR: storageDir,
      BIND_ADDR: '127.0.0.1',
      PORT: String(backendPort),
      FILE_TTL_HOURS: '0',
      JOB_TIMEOUT_SECS: '60',
      MAX_CONCURRENT_JOBS: '1',
      CORS_ALLOW_ORIGINS: frontendUrl,
      RUST_LOG: 'warn',
    },
  })
  await waitForUrl(`${backendUrl}/api/health`, backend, 'Rust backend')

  const vite = spawnServer(
    join(frontendRoot, 'node_modules', '.bin', 'vite'),
    ['--config', 'vite.real.config.ts', '--host', '127.0.0.1', '--port', String(frontendPort), '--strictPort'],
    {
      cwd: frontendRoot,
      env: { ...process.env, REAL_COMPOSITION_BACKEND_URL: backendUrl },
    },
  )
  await waitForUrl(frontendUrl, vite, 'Vite frontend')

  await spawnBounded(
    join(frontendRoot, 'node_modules', '.bin', 'playwright'),
    ['test', 'real-composition.spec.ts', '--config', 'playwright.real.config.ts', '--project', 'chromium-real'],
    {
      cwd: frontendRoot,
      env: {
        ...process.env,
        REAL_COMPOSITION_BASE_URL: frontendUrl,
        REAL_COMPOSITION_FIXTURE: fixturePath,
      },
    },
    150_000,
  ).then((output) => process.stdout.write(output))
} finally {
  await cleanup()
}
