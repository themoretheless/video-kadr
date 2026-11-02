import type {NativeScopeKind,NativeScopeOptions} from '$lib/api'
export const SCOPE_PREFERENCES_KEY='video-kadr.scopes.v1'
export interface ScopePreferences {
 shown?:NativeScopeKind[]
 kind:NativeScopeKind
 matrix:NonNullable<NativeScopeOptions['matrix']>|'auto'
 waveformType:NonNullable<NativeScopeOptions['waveformType']>
 paradeType:NonNullable<NativeScopeOptions['paradeType']>
 brightness:NonNullable<NativeScopeOptions['brightness']>
 scale:'bits8'|'float'|'hdr'
 targets:'75'|'100'
 clamp:boolean
}
const defaults:ScopePreferences={kind:'waveform',matrix:'auto',waveformType:'rgb',paradeType:'rgb',brightness:'normal',scale:'bits8',targets:'75',clamp:true}
const allowed={kind:['waveform','parade','histogram','vectorscopeYuv','vectorscopeHls'],matrix:['auto','Bt601','Bt709','Bt2020Ncl'],waveformType:['rgb','luma','yc','ycNoChroma'],paradeType:['rgb','yuv','rgbWhite'],brightness:['dimmed','normal','bright'],scale:['bits8','float','hdr'],targets:['75','100']} as const
export function decodeScopePreferences(value:unknown):ScopePreferences {
 const result={...defaults}
 if(!value||typeof value!=='object'||Array.isArray(value))return result
 const input=value as Record<string,unknown>
 for(const key of Object.keys(allowed) as (keyof typeof allowed)[]) {
  const candidate=input[key]
  if(typeof candidate==='string'&&(allowed[key] as readonly string[]).includes(candidate))Object.assign(result,{[key]:candidate})
 }
 if(Array.isArray(input.shown)&&input.shown.length<=5){const shown=[...new Set(input.shown.filter((v):v is NativeScopeKind=>typeof v==='string'&&(allowed.kind as readonly string[]).includes(v)))];if(shown.length)result.shown=shown}
 if(typeof input.clamp==='boolean')result.clamp=input.clamp
 return result
}
export function readScopePreferences():ScopePreferences {
 try {const raw=window.localStorage.getItem(SCOPE_PREFERENCES_KEY);return raw&&raw.length<=4096?decodeScopePreferences(JSON.parse(raw)):{...defaults}}catch{return {...defaults}}
}
export function writeScopePreferences(value:ScopePreferences):void {
 try {window.localStorage.setItem(SCOPE_PREFERENCES_KEY,JSON.stringify(decodeScopePreferences(value)))}catch{/* Controls remain usable when browser storage is unavailable. */}
}
