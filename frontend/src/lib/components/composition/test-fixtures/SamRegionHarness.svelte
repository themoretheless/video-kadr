<script lang="ts">
 import Tool from '../SamRegionTool.svelte'
 import type {SavedSamRegion} from '$lib/api'
 let {onready,onbusy}:{onready:(entry:SavedSamRegion)=>void;onbusy:(value:boolean)=>void}=$props()
 let token=$state('owner'),sourceId=$state('input'),targetKey=$state('clip')
</script>
<button onclick={()=>token='other'}>Сменить аккаунт SAM</button>
<button onclick={()=>sourceId='another'}>Сменить исходник SAM</button>
<button onclick={()=>targetKey='another'}>Сменить клип SAM</button>
<Tool {token} {sourceId} {targetKey} projectId="project" sourceHash={'a'.repeat(64)} points={[[0.4,0.5]]} {onready} {onbusy}/>
