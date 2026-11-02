<script lang="ts">
 import Panel from '../PersonVideoPanel.svelte'
 import {videoRegionRecipe} from './videoRegionRecipe'
 import type {SavedSamVideoRegion} from '$lib/api'
 import type {VideoMaskAttachment} from '$lib/composition/types'
 let {onattached}:{onattached:(entry:SavedSamVideoRegion,expected:VideoMaskAttachment)=>void}=$props()
 let redoSnapshot:VideoMaskAttachment|undefined
 let token=$state('owner'),targetKey=$state('clip'),context=$state({version:1}),snapshot=$state(videoRegionRecipe().projectOrigin!.snapshot)
 async function attach(entry:SavedSamVideoRegion,expected:VideoMaskAttachment){onattached(entry,expected);snapshot={...snapshot,trackedRegions:[...(snapshot.trackedRegions??[]),entry.snapshot]};context={version:2}}
</script>
<button onclick={()=>token='other'}>Сменить аккаунт подготовки</button>
<button onclick={()=>targetKey='another'}>Сменить клип подготовки</button>
<button onclick={()=>context={version:3}}>Изменить документ подготовки</button>
<button onclick={()=>{redoSnapshot=snapshot;snapshot=videoRegionRecipe().projectOrigin!.snapshot;context={version:4}}}>Undo области</button>
<button onclick={()=>{if(redoSnapshot){snapshot=redoSnapshot;context={version:5}}}}>Redo области</button>
<output data-region-count={snapshot.trackedRegions?.length??0}></output>
<Panel {token} {targetKey} {context} projectId="project" sourceId="original" width={37} height={23} savedMask={{clipId:'clip',compositionId:'root',snapshot}} onaccepted={()=>{}} ontracked={attach}/>
