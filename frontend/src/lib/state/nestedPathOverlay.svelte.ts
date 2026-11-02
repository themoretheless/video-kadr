import type { NativeNestedProject } from '$lib/api'
export interface PathOverlaySelection { owner:symbol; base:NativeNestedProject; compositionId:string; clipId:string; token:string; generation:number; projectId:string|null }
class OverlayState { value=$state.raw<PathOverlaySelection|null>(null) }
export const nestedPathOverlay=new OverlayState()
export function clearPathOverlay(owner:symbol){if(nestedPathOverlay.value?.owner===owner)nestedPathOverlay.value=null}
