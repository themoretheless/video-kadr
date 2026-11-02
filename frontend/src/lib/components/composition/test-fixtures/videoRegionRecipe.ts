import type {VideoCorrectionRecipe} from '$lib/api'
import type {SamVideoRegionAttachment,VideoMaskAttachment} from '$lib/composition/types'
export function videoRegionRecipe():VideoCorrectionRecipe {
 const points:[[number,number]]=[[.5,.5]]
 const region:SamVideoRegionAttachment={sourceId:'original',maskSourceId:'tracked',sourceHash:'a'.repeat(64),maskHash:'d'.repeat(64),maskFileHash:'e'.repeat(64),encoderHash:'f'.repeat(64),decoderHash:'0'.repeat(64),runtimeHash:'1'.repeat(64),promptHash:'8e0f6ce7be723f24b418232b01f7fdb4ddac66f10dcab24c069812ec3aa39e62',points,width:37,height:23,frames:4,frameRate:{numerator:4,denominator:1},seedFrame:1,trackedFrames:4,forwardLostAt:null,backwardLostAt:null}
 const snapshot:VideoMaskAttachment={sourceId:'original',maskSourceId:'automatic',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),width:37,height:23,maskWidth:512,maskHeight:320,frames:4,frameRate:{numerator:4,denominator:1},corrections:[],trackedRegions:[region]}
 const plane={sourceId:region.sourceId,maskSourceId:region.maskSourceId,sourceHash:region.sourceHash,maskHash:'3'.repeat(64),maskFileHash:region.maskFileHash,encoderHash:region.encoderHash,decoderHash:region.decoderHash,runtimeHash:region.runtimeHash,promptHash:region.promptHash,points}
 return {baseJobId:'original-job',maskHash:snapshot.maskHash,projectOrigin:{projectId:'project',clipId:'clip',compositionId:'root',snapshot},corrections:[{frame:1,correction:{custom:true,strokes:[{tool:'smartBrush',size:.1,points,region:plane}],feather:0}}]}
}
