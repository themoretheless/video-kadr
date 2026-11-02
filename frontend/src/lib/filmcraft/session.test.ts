import { expect, it } from 'vitest'
import { encodeFilmcraftSession, parseFilmcraftSession, type FilmcraftSession } from './session'
const session: FilmcraftSession = { format: 'video-kadr.filmcraft-session', version: 1, documentText: '{"opaque":{"tick":9007199254740993,"id":18446744073709551615}}', sequenceId: '18446744073709551615', sources: { '9007199254740993': 'library-audio' }, start: 0.125, end: 2.75, audioFormat: 'aiff', bits: 24, channels: 2 }
it('roundtrips source text and exact ID strings alongside all session settings', () => {
  expect(parseFilmcraftSession(encodeFilmcraftSession(session))).toEqual(session)
  expect(parseFilmcraftSession(session.documentText)).toBeNull()
})
it('refuses unsupported sessions, invalid ranges and out-of-u64 IDs', () => {
  for (const patch of [{ version: 2 }, { end: 0 }, { sequenceId: '18446744073709551616' }, { sources: { bad: 'file' } }, { bits: 32 }, { channels: 6 }]) {
    expect(() => parseFilmcraftSession(JSON.stringify({ ...session, ...patch }))).toThrow()
  }
})

it('roundtrips transition durations and effect choices without changing source text or exact frame strings', () => {
 const transitionSettings = { video: { effect: 'push', duration: { value: '9007199254740993', unit: 'frames' as const } }, audio: { effect: 'constant_power', duration: { value: '1.25', unit: 'seconds' as const } }, single: { value: '0.5', unit: 'seconds' as const } }
 expect(parseFilmcraftSession(encodeFilmcraftSession({ ...session, transitionSettings }))).toEqual({ ...session, transitionSettings })
 for (const settings of [null, {}, { ...transitionSettings, single: { value: 'Infinity', unit: 'seconds' } }, { ...transitionSettings, single: { value: '-1', unit: 'frames' } }, { ...transitionSettings, single: { value: ' ', unit: 'frames' } }, { ...transitionSettings, single: { value: '1', unit: 'minutes' } }, { ...transitionSettings, video: { effect: '../unknown', duration: transitionSettings.video.duration } }]) expect(() => parseFilmcraftSession(JSON.stringify({ ...session, transitionSettings: settings }))).toThrow()
})
