import { readFile } from 'node:fs/promises'
import { performance } from 'node:perf_hooks'
import assert from 'node:assert/strict'
const wasm = await readFile(new URL('../../backend/preview-core/target/wasm32-unknown-unknown/release/video_kadr_preview_core.wasm',import.meta.url))
const module = await WebAssembly.compile(wasm)
const keys = Array.from({ length: 4096 },(_,index) => ({ t_us: index*1000,value: { r: index%256,g: 128,b: 255-index%256,a: 255 },in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } }))
const input = new TextEncoder().encode(JSON.stringify({ track: { keys,extrapolate: { before: 'Hold',after: 'Hold' } },default: { r: 0,g: 0,b: 0,a: 255 },times_us: [] }))
const instance = await WebAssembly.instantiate(module)
const api = instance.exports
const start = performance.now()
api.color_begin()
for (const byte of input) assert.equal(api.color_push(byte),0)
assert.equal(api.color_prepare(),0)
const preparationMs = performance.now()-start
for (let index=0;index<1000;index++) assert.equal(api.color_at(index*4095),0)
const runs = []
let checksum = 0
for (let run=0;run<5;run++) {
  const start = performance.now()
  for (let index=0;index<10000;index++) {
    assert.equal(api.color_at((index*397)%4095000),0)
    checksum ^= api.color_packed()
  }
  runs.push(performance.now()-start)
}
console.log(JSON.stringify({ keys: keys.length,samplesPerRun: 10000,preparationMs,runsMs: runs,linearMemoryBytes: api.memory.buffer.byteLength,checksum }))
