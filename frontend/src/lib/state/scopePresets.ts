import type {NativeScopeKind,NativeScopeOptions} from '$lib/api'
// Layout/settings tuples from Filmcraft panel_state.rs SCOPE_PRESETS (MIT).
// These select native analyzer modes; no signal processing occurs here.
export interface ScopePreset {
 name:string
 shown:readonly NativeScopeKind[]
 waveformType:NonNullable<NativeScopeOptions['waveformType']>
 paradeType:NonNullable<NativeScopeOptions['paradeType']>
 scale:'bits8'|'float'
 clamp:boolean
}
export const scopePresets:readonly ScopePreset[]=[
 {name:'Waveform (RGB)',shown:['waveform'],waveformType:'rgb',paradeType:'rgb',scale:'bits8',clamp:true},
 {name:'Waveform (Luma)',shown:['waveform'],waveformType:'luma',paradeType:'rgb',scale:'bits8',clamp:true},
 {name:'Parade (RGB)',shown:['parade'],waveformType:'rgb',paradeType:'rgb',scale:'bits8',clamp:true},
 {name:'Vectorscope (YUV)',shown:['vectorscopeYuv'],waveformType:'rgb',paradeType:'rgb',scale:'bits8',clamp:true},
 {name:'Vectorscope + Waveform YC',shown:['vectorscopeYuv','waveform'],waveformType:'yc',paradeType:'rgb',scale:'bits8',clamp:true},
 {name:'Four Scopes (YUV, float, no clamp)',shown:['vectorscopeYuv','histogram','parade','waveform'],waveformType:'yc',paradeType:'yuv',scale:'float',clamp:false},
 {name:'Four Scopes (RGB, 8 bit)',shown:['vectorscopeYuv','histogram','parade','waveform'],waveformType:'rgb',paradeType:'rgb',scale:'bits8',clamp:true},
 {name:'HLS Vectorscope + Histogram',shown:['vectorscopeHls','histogram'],waveformType:'rgb',paradeType:'rgb',scale:'bits8',clamp:true},
]
