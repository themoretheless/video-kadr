<script lang="ts">
 import Graph from '../CurvePointGraph.svelte'
 import type{CurveGesture,CurvePointDraft}from'../CurvePointGraph.svelte'
 import type{CurvePoints}from'$lib/composition/concatCurveAuthoring'
 let{onchange,onpreview}:{onchange:(points:CurvePoints,meta:CurveGesture)=>void;onpreview:(draft:CurvePointDraft|null,gesture:object)=>void}=$props()
 let points:CurvePoints=$state.raw([[0,0],[0.5,0.5],[1,1]])
 let scope:object=$state.raw({version:1}),clock=$state(0),session=$state('first'),context=$state('root:luma'),disabled=$state(false)
 export function replaceScope(){scope={version:2}}
 export function replacePoints(){points=[[0,0],[0.5,0.5],[1,1]]}
 export function seek(){clock+=1}
 export function logout(){session='second'}
 export function selectChannel(){context='root:blue'}
 export function lock(){disabled=true}
</script>
<Graph {points} {scope} {clock} {session} {context} {disabled} {onchange} {onpreview} label="Luma" path="M0 200L200 0"/>
