import type { FilmcraftMonitorFrame } from '$lib/api'
export function decodeMonitorFrame(frame: FilmcraftMonitorFrame): Uint8ClampedArray {
  const { width, height, rgbaHex } = frame
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 || width > 480 || height > 270 || rgbaHex.length !== width * height * 8 || !/^[0-9a-f]+$/i.test(rgbaHex)) throw new Error('Некорректный кадр монитора')
  const bytes = new Uint8ClampedArray(width * height * 4)
  for (let i = 0; i < bytes.length; i++) bytes[i] = Number.parseInt(rgbaHex.slice(i * 2, i * 2 + 2), 16)
  return bytes
}
