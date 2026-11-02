import {exactCaptionTicks} from './captionTime'
/** Validate the persisted native caption contract before saving or rendering. */
export function validateCaptionTracks(value: unknown, trackIds: Set<string>, clipIds: Set<string>, add: (code: string, path: string, message: string) => void): void {
  if (value === undefined) return
  const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v)
  const id = (v: unknown): v is string => typeof v === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(v) && !['__proto__','prototype','constructor'].includes(v)
  const color = (v: unknown) => typeof v === 'string' ? /^#?[0-9a-f]{6}([0-9a-f]{2})?$/i.test(v.trim()) : Array.isArray(v) && v.length === 4 && v.every(n => Number.isInteger(n) && n >= 0 && n <= 255)
  if (!Array.isArray(value)) { add('captions','captionTracks','Caption tracks must be an array'); return }
  for (const [i, track] of value.entries()) {
    const path = `captionTracks[${i}]`
    const fail = (message: string) => add('captions',path,message)
    if (!record(track)) { fail('Invalid caption track'); continue }
    if (!id(track.id) || trackIds.has(track.id)) fail('Caption track needs a unique stable id')
    else trackIds.add(track.id)
    if (typeof track.name !== 'string') fail('Caption track name must be text')
    if (track.format !== undefined && !['Subtitle','CEA-608','CEA-708','Teletext'].includes(String(track.format))) fail('Invalid caption stream format')
    if (track.language !== undefined && typeof track.language !== 'string') fail('Invalid caption language')
    if (track.vttBlocks !== undefined && (!Array.isArray(track.vttBlocks) || !track.vttBlocks.every(block => typeof block === 'string'))) fail('Invalid WebVTT blocks')
    for (const flag of ['hidden','locked','syncLock']) if (track[flag] !== undefined && typeof track[flag] !== 'boolean') fail(`Invalid ${flag}`)
    if (track.style !== undefined) {
      if (!record(track.style)) fail('Invalid caption style')
      else {
        const style = track.style
        if (style.font !== undefined && (typeof style.font !== 'string' || !style.font.trim())) fail('Caption font must not be empty')
        for (const [key,min,max] of [['size',4,400],['margin',0,0.45],['lineSpacing',0.8,4],['outline',0,40]] as const) {
          const n = style[key]
          if (n !== undefined && (typeof n !== 'number' || !Number.isFinite(n) || n < min || n > max)) fail(`Invalid caption ${key}`)
        }
        for (const key of ['color','backgroundColor','outlineColor']) if (style[key] !== undefined && !color(style[key])) fail(`Invalid caption ${key}`)
        if (style.background !== undefined && typeof style.background !== 'boolean') fail('Invalid caption background')
        if (style.align !== undefined && !['left','center','right'].includes(String(style.align))) fail('Invalid caption alignment')
        if (style.anchor !== undefined && !['top','middle','bottom'].includes(String(style.anchor))) fail('Invalid caption anchor')
      }
    }
    if (!Array.isArray(track.clips)) { fail('Caption clips must be an array'); continue }
    let previousEnd = 0n
    for (const cue of track.clips) {
      if (!record(cue)) { fail('Invalid caption cue'); continue }
      if (!id(cue.id) || clipIds.has(cue.id)) fail('Caption cue needs a unique stable id')
      else clipIds.add(cue.id)
      const start = cue.timelineStartTick, end = cue.timelineEndTick
      try {
       if((typeof start!=='number'&&typeof start!=='string')||(typeof end!=='number'&&typeof end!=='string'))throw new Error()
       const s=exactCaptionTicks(start),e=exactCaptionTicks(end)
       if(s<0n||e<=s)throw new Error()
       if(s<previousEnd)fail('Captions must be sorted and non-overlapping');previousEnd=e
      }catch{fail('Invalid caption placement')}
      if (typeof cue.text !== 'string' || new TextEncoder().encode(cue.text).length > 1_000_000) fail('Invalid caption text')
      if (cue.speaker !== undefined && cue.speaker !== null && typeof cue.speaker !== 'string') fail('Invalid caption speaker')
      if (cue.cueId !== undefined && cue.cueId !== null && typeof cue.cueId !== 'string') fail('Invalid caption file cue identifier')
      if (cue.settings !== undefined && typeof cue.settings !== 'string') fail('Invalid caption settings')
    }
  }
}
