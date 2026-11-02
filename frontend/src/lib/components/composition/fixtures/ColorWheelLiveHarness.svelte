<script lang="ts">
 import Inspector from '../NestedConcatColorWheelsInspector.svelte'
 import Preview from '../NestedCompositionPreview.svelte'
 import{nestedComposition}from'$lib/state/nestedComposition.svelte'
 let {showPreview=false}:{showPreview?:boolean}=$props()
 let token=$state('first')
 export function changeSession(){token='second'}
</script>
<Inspector {token}/>
{#if showPreview&&nestedComposition.document}
 <Preview project={nestedComposition.document} timeUs={nestedComposition.timeUs} renderedOutputs={nestedComposition.renderedOutputs} compositionId={nestedComposition.activeCompositionId??nestedComposition.document.root} {token} effectDraft label="Live wheel frame"/>
 <Preview project={nestedComposition.document} timeUs={nestedComposition.timeUs} renderedOutputs={nestedComposition.renderedOutputs} compositionId={nestedComposition.activeCompositionId??nestedComposition.document.root} {token} label="Committed reference frame"/>
{/if}
