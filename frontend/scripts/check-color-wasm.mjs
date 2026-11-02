import { readFile } from 'node:fs/promises'
import assert from 'node:assert/strict'
const wasm = await readFile(new URL('../../backend/preview-core/target/wasm32-unknown-unknown/release/video_kadr_preview_core.wasm',import.meta.url))
const { instance } = await WebAssembly.instantiate(wasm)
const api = instance.exports
function sample(request) {
  api.color_begin()
  for (const byte of new TextEncoder().encode(JSON.stringify(request))) assert.equal(api.color_push(byte),0)
  const status = api.color_sample()
  const bytes = Uint8Array.from({ length: api.color_output_len() },(_,index) => api.color_output_byte(index))
  const output = new TextDecoder().decode(bytes)
  if (status) throw new Error(output)
  return JSON.parse(output).colors
}
const fixture = JSON.parse(await readFile(new URL('../../backend/tests/fixtures/weftcut-animated-color.json',import.meta.url),'utf8'))
let count = 0
for (const test of fixture.cases) {
  const track = test.track
  const keys = track.mode === 'Static' ? [{ t_us: 0,value: track.value,in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } }] : track.value.map(key => ({ t_us: key.t_us,value: key.value,in_: [key.in.x,key.in.y],out: [key.out.x,key.out.y],segment: key.segment }))
  const colors = sample({ track: { keys,extrapolate: track.extrapolate ?? { before: 'Hold',after: 'Hold' } },default: fixture.default,times_us: test.samples.map(value => value.t_us) })
  api.color_begin()
  const request = { track: { keys,extrapolate: track.extrapolate ?? { before: 'Hold',after: 'Hold' } },default: fixture.default,times_us: [] }
  for (const byte of new TextEncoder().encode(JSON.stringify(request))) assert.equal(api.color_push(byte),0)
  assert.equal(api.color_prepare(),0)
  test.samples.forEach((value,index) => {
    assert.equal(api.color_at(value.t_us),0)
    const packed = api.color_packed() >>> 0
    assert.deepEqual({ r: packed >>> 24,g: (packed >>> 16) & 255,b: (packed >>> 8) & 255,a: packed & 255 },colors[index])
  })
  for (const invalid of [NaN,Infinity,0.5,Number.MAX_SAFE_INTEGER+1]) assert.equal(api.color_at(invalid),1)
  test.samples.forEach((value,index) => {
    for (const channel of ['r','g','b','a']) assert.ok(Math.abs(colors[index][channel]-value.expect[channel]) <= 1,`${test.name} ${channel}`)
    count++
  })
}
assert.throws(() => sample({ track: { keys: [],extrapolate: { before: 'Hold',after: 'Hold' } },default: { r: 256,g: 0,b: 0,a: 255 },times_us: [0] }))
console.log(`WASM color evaluator: ${fixture.cases.length} golden cases / ${count} samples and invalid-channel refusal passed (${wasm.length} bytes).`)
// Multiple resident tracks must coexist in one memory and release explicitly.
function upload(request) {
  api.color_begin()
  for (const byte of new TextEncoder().encode(JSON.stringify(request))) assert.equal(api.color_push(byte),0)
}
const memoryBefore = api.memory.buffer.byteLength
for (let id=1;id<=512;id++) {
  upload({ track: { keys: [],extrapolate: { before: 'Hold',after: 'Hold' } },default: { r: id%256,g: (id*3)%256,b: 20,a: 255 },times_us: [] })
  assert.equal(api.color_track_prepare(id),0)
}
assert.equal(api.color_track_count(),512)
for (let id=1;id<=512;id++) {
  assert.equal(api.color_track_at(id,123456),0)
  assert.equal(api.color_packed() >>> 0,(((id%256)<<24)|(((id*3)%256)<<16)|(20<<8)|255) >>> 0)
}
const memoryLoaded = api.memory.buffer.byteLength
api.color_track_drop(7)
api.color_track_drop(7)
assert.equal(api.color_track_at(7,0),1)
assert.equal(api.color_track_at(8,0),0)
upload({ invalid: true })
assert.equal(api.color_track_prepare(8),1)
assert.equal(api.color_track_at(8,0),1)
assert.equal(api.color_track_at(9,0),0)
for (let id=1;id<=512;id++) api.color_track_drop(id)
assert.equal(api.color_track_count(),0)
assert.equal(api.color_track_at(1,0),1)
console.log(`Shared WASM: 512 independent tracks, failed replacement invalidation and explicit drop passed; memory ${memoryBefore} → ${memoryLoaded} bytes (high-water allocation).`)
