<script lang="ts">
 import Preview from '../NestedCompositionPreview.svelte'
 import type{NativeNestedProject}from'$lib/api'
 let {base}:{base:NativeNestedProject}=$props()
 let token=$state('first')
 let timeUs=$state(0),playing=$state(false),playbackEpoch=$state(0)
 export function playTime(time:number){playing=true;timeUs=time}
 export function seekTime(time:number){timeUs=time;playbackEpoch++}
 export function changeSession(){token='second'}
</script>
<Preview project={base} {timeUs} {playing} {playbackEpoch} renderedOutputs={{}} {token} effectDraft/>
