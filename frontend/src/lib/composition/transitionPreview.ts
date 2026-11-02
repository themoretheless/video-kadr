import type {NativeNestedProject} from '$lib/api'
import type {Composition} from './types'
import {buildCompositionWireDocument} from './payload'
import {independentTransitionPreviewReason} from './transitionExport'

/** Reuse the native compositor wire contract without applying export/audio eligibility. */
export function buildIndependentTransitionPreviewProject(composition:Composition):NativeNestedProject|null {
 if(!composition.tracks.some(track=>track.kind==='video'&&!track.hidden&&track.transitions?.some(transition=>transition.timeline)))return null
 const reason=independentTransitionPreviewReason(composition)
 if(reason)throw new Error(reason)
 return {schemaVersion:1,root:'editor',compositions:{editor:{content:buildCompositionWireDocument(composition),instances:[]}}}
}
