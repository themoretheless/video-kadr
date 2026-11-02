import type {NativeNestedProject}from'$lib/api'
export interface EffectPreviewDraft {owner:symbol;gesture:object;base:NativeNestedProject;project:NativeNestedProject;timeUs:number;compositionId:string;token:string}
class PreviewState {value=$state.raw<EffectPreviewDraft|null>(null)}
export const nestedEffectPreview=new PreviewState()
export function clearEffectPreview(owner:symbol,gesture?:object){const value=nestedEffectPreview.value;if(value?.owner===owner&&(!gesture||value.gesture===gesture))nestedEffectPreview.value=null}
export function resolveEffectPreview(base:NativeNestedProject,timeUs:number,compositionId:string|undefined,token:string):EffectPreviewDraft|null {const d=nestedEffectPreview.value;return d&&d.base===base&&d.timeUs===timeUs&&d.compositionId===(compositionId??base.root)&&d.token===token?d:null}
