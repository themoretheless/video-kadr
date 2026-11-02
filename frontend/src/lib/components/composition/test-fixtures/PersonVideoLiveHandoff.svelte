<script lang="ts">
 import Panel from '../PersonVideoPanel.svelte'
 import {nestedComposition,attachNestedVideoRegion,replaceNestedVideoCutoutMedia} from '$lib/state/nestedComposition.svelte'
 let {token,projectId}:{token:string;projectId:string}=$props()
 const project=$derived(nestedComposition.document)
 const node=$derived(project?.compositions.root)
 const pair=$derived(node?.content.metadata?.mediaVersions?.clip)
</script>
{#if project&&node&&pair?.videoCutout}
 <Panel {token} {projectId} sourceId={pair.copySourceId} targetKey="root/clip/clip-audio" width={37} height={23} context={project} savedMask={{clipId:'clip',compositionId:'root',snapshot:pair.videoCutout}} ontracked={async(entry,expected)=>{await attachNestedVideoRegion('root','clip',expected,entry)}} onaccepted={async entry=>{await replaceNestedVideoCutoutMedia('root',['clip'],entry)}}/>
{/if}
