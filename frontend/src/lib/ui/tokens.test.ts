import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const themes = parseThemes(readFileSync('src/lib/ui/tokens.css', 'utf8'))

describe('semantic design token contract', () => {
  it('is explicitly versioned and exposes the same semantic colors in both themes', () => {
    expect(themes.dark['design-token-version']).toBe('1')
    const semantic = ['bg', 'panel', 'panel-2', 'panel-3', 'border', 'border-strong', 'text', 'muted', 'faint', 'accent', 'accent-2', 'danger', 'ok', 'warn']
    for (const name of semantic) {
      expect(themes.dark[name], `dark --${name}`).toBeTruthy()
      expect(themes.light[name], `light --${name}`).toBeTruthy()
    }
  })

  it.each(['dark', 'light'] as const)('%s body and muted text meet WCAG AA', (theme) => {
    expect(contrast(themes[theme].text, themes[theme].bg)).toBeGreaterThanOrEqual(4.5)
    expect(contrast(themes[theme].muted, themes[theme].panel)).toBeGreaterThanOrEqual(4.5)
  })
})

function parseThemes(css: string): Record<'dark' | 'light', Record<string, string>> {
  const blocks = [...css.matchAll(/:root(?:\[data-theme="light"\])?\s*\{([^}]*)\}/g)]
  const parse = (body: string) => Object.fromEntries(
    [...body.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((match) => [match[1], match[2].trim()]),
  )
  return { dark: parse(blocks[0]?.[1] ?? ''), light: parse(blocks[1]?.[1] ?? '') }
}

function contrast(foreground: string, background: string): number {
  const high = Math.max(luminance(foreground), luminance(background))
  const low = Math.min(luminance(foreground), luminance(background))
  return (high + 0.05) / (low + 0.05)
}

function luminance(hex: string): number {
  const channels = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex)
  if (!channels) throw new Error(`Expected an opaque six-digit color, received ${hex}`)
  return channels.slice(1).map((channel) => Number.parseInt(channel, 16) / 255)
    .map((value) => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4)
    .reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0)
}
