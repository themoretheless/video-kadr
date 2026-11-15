export const sceneDescriptionLanguages = {
  ru: 'Русский',
  en: 'English',
  'zh-hans': '简体中文',
  'zh-hant': '繁體中文',
  ja: '日本語',
  ko: '한국어',
  es: 'Español',
  fr: 'Français',
  de: 'Deutsch',
  pt: 'Português',
} as const

export type SceneDescriptionLanguage = keyof typeof sceneDescriptionLanguages

export function parseSceneDescriptionLanguage(value: unknown): SceneDescriptionLanguage | undefined {
  if (typeof value !== 'string') return undefined
  const tag = value.trim().toLowerCase()
  if (tag.length > 35 || !/^[a-z]{2,3}(?:-[a-z0-9]{2,8})*$/.test(tag)) return undefined
  const [base, variant] = tag.split('-')
  if (base === 'zh') return ['hant', 'tw', 'hk', 'mo'].includes(variant ?? '') ? 'zh-hant' : 'zh-hans'
  return base !== undefined && Object.hasOwn(sceneDescriptionLanguages, base) ? base as SceneDescriptionLanguage : undefined
}
