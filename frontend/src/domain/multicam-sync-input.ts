import type { MulticamRate } from './multicam'

export interface ParsedSmpteTimecode {
  startFrame: number
  rate: MulticamRate
  dropFrame: boolean
}

export function parseSyncClock(value: string, timeBase: number, durationTicks: number): number {
  if (!Number.isSafeInteger(timeBase) || timeBase <= 0 || !Number.isSafeInteger(durationTicks) || durationTicks <= 0) {
    throw new Error('Некорректная шкала времени')
  }
  const match = /^(?:(\d{1,2}):)?(\d{1,2}):(\d{2})(?:\.(\d{1,3}))?$/.exec(value.trim())
  if (!match) throw new Error('Введите sync-точку как чч:мм:сс.мс')
  const hours = Number(match[1] ?? 0); const minutes = Number(match[2]); const seconds = Number(match[3])
  const milliseconds = Number((match[4] ?? '').padEnd(3, '0') || 0)
  if (minutes > 59 || seconds > 59 || hours > 99) throw new Error('Sync-точка вне допустимого формата')
  const ticks = Math.round((hours * 3600 + minutes * 60 + seconds + milliseconds / 1000) * timeBase)
  if (!Number.isSafeInteger(ticks) || ticks < 0 || ticks >= durationTicks) throw new Error('Sync-точка находится вне источника')
  return ticks
}

export function rateFromKey(value: string): MulticamRate {
  const rates: Record<string, MulticamRate> = {
    '23.976': { numerator: 24_000, denominator: 1_001 },
    '24': { numerator: 24, denominator: 1 },
    '25': { numerator: 25, denominator: 1 },
    '29.97': { numerator: 30_000, denominator: 1_001 },
    '30': { numerator: 30, denominator: 1 },
    '50': { numerator: 50, denominator: 1 },
    '59.94': { numerator: 60_000, denominator: 1_001 },
    '60': { numerator: 60, denominator: 1 },
  }
  const rate = rates[value]
  if (!rate) throw new Error('Неподдерживаемая частота таймкода')
  return rate
}

export function parseSmpteTimecode(value: string, rateKey: string, dropFrame: boolean): ParsedSmpteTimecode {
  const rate = rateFromKey(rateKey)
  const match = /^(\d{2}):(\d{2}):(\d{2})[:;](\d{2})$/.exec(value.trim())
  if (!match) throw new Error('Введите таймкод как ЧЧ:ММ:СС:КК')
  const hours = Number(match[1]); const minutes = Number(match[2]); const seconds = Number(match[3]); const frames = Number(match[4])
  const nominal = Math.round(rate.numerator / rate.denominator)
  if (hours > 23 || minutes > 59 || seconds > 59 || frames >= nominal) throw new Error('Таймкод вне допустимого диапазона')
  const dropSupported = (rate.numerator === 30_000 || rate.numerator === 60_000) && rate.denominator === 1_001
  if (dropFrame && !dropSupported) throw new Error('Drop-frame доступен только для 29.97 и 59.94 fps')
  const totalMinutes = hours * 60 + minutes
  const droppedPerMinute = nominal === 60 ? 4 : 2
  if (dropFrame && minutes % 10 !== 0 && seconds === 0 && frames < droppedPerMinute) {
    throw new Error('Этот номер кадра пропускается в drop-frame таймкоде')
  }
  const nominalFrames = ((hours * 3600 + minutes * 60 + seconds) * nominal) + frames
  const dropped = dropFrame ? droppedPerMinute * (totalMinutes - Math.floor(totalMinutes / 10)) : 0
  return { startFrame: nominalFrames - dropped, rate, dropFrame }
}

export function defaultTimecodeRate(fps: number | null | undefined): string {
  const candidates = [23.976, 24, 25, 29.97, 30, 50, 59.94, 60]
  if (typeof fps !== 'number' || !Number.isFinite(fps)) return '25'
  return String(candidates.reduce((best, candidate) => Math.abs(candidate - fps) < Math.abs(best - fps) ? candidate : best))
}
