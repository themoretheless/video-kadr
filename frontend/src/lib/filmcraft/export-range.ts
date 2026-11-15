function secondsToTicks(value: string) {
  const parts = /^(\d{1,5})(?:[.,](\d{1,12}))?$/.exec(value.trim())
  if (!parts) return undefined
  const fraction = BigInt((parts[2] ?? '').padEnd(12, '0'))
  const ticks = BigInt(parts[1]) * 254_016_000_000n
    + (fraction * 254_016_000_000n + 500_000_000_000n) / 1_000_000_000_000n
  return ticks <= 86_400n * 254_016_000_000n ? ticks : undefined
}
export function exactExportRange(first: string, last: string) {
  const start = secondsToTicks(first), end = secondsToTicks(last)
  if (start === undefined || end === undefined || end <= start) return undefined
  return { startTicks: start.toString(), endTicks: end.toString() }
}
