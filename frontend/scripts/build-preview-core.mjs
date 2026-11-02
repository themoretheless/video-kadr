import { spawn } from 'node:child_process'
import { copyFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
const manifest = fileURLToPath(new URL('../../backend/preview-core/Cargo.toml',import.meta.url))
const build = spawn('cargo',['build','--locked','--manifest-path',manifest,'--target','wasm32-unknown-unknown','--release'],{ stdio: 'inherit' })
await new Promise((done,fail) => { build.once('error',fail);build.once('exit',code => code === 0 ? done() : fail(new Error(`Preview core build exited ${code}`))) })
await copyFile(new URL('../../backend/preview-core/target/wasm32-unknown-unknown/release/video_kadr_preview_core.wasm',import.meta.url),new URL('../src/lib/native/color-preview.wasm',import.meta.url))
