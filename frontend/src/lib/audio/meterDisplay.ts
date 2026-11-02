// Display envelopes match WeftCut useMasterMeterDisplay; raw readings stay exact.
const SILENCE = -120
interface Envelope { value: number; target: number; releaseAt: number; updatedAt: number }
function initial(now: number): Envelope { return { value: SILENCE, target: SILENCE, releaseAt: now, updatedAt: now } }
function receive(state: Envelope, target: number, now: number, hold: number): void {
  state.target = Number.isFinite(target) ? target : SILENCE
  if (state.target <= SILENCE) state.releaseAt = now
  if (state.target >= state.value) { state.value = state.target; state.releaseAt = now + hold; state.updatedAt = now }
}
function release(state: Envelope, now: number, rate: number): void {
  const elapsed = Math.max(0, now - Math.max(state.updatedAt, state.releaseAt))
  state.updatedAt = now
  state.value = Math.max(state.target, state.value - elapsed * (state.target <= SILENCE ? 240 : rate) / 1000)
  if (state.target <= SILENCE && state.value <= -60) state.value = SILENCE
}
export function createMeterDisplay(now = 0) {
  const rms = initial(now), level = initial(now), peak = initial(now)
  const advance = (time: number) => {
    release(rms, time, 24); release(level, time, 36); release(peak, time, 18)
    peak.value = Math.max(peak.value, level.value)
  }
  return {
    receive(sample: { rmsDb: number; peakDb: number }, time: number) {
      advance(time)
      receive(rms, sample.rmsDb, time, 300)
      receive(level, sample.peakDb, time, 0)
      receive(peak, sample.peakDb, time, 1000)
    },
    advance,
    resetPeak(time: number) { peak.value = level.value; peak.releaseAt = time + 1000 },
    get value() { return { rmsDb: rms.value, levelDb: level.value, peakDb: peak.value } },
    get settled() { return rms.value === rms.target && level.value === level.target && peak.value === peak.target },
  }
}
