<script lang="ts">
 import ProjectFonts from '../../src/lib/components/composition/ProjectFonts.svelte'
 import {nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '../../src/lib/state/nestedComposition.svelte'
 function showMissing(){
  const project=JSON.parse(JSON.stringify(nestedComposition.document))
  const layer=project.compositions.unused.content.tracks[0].clips[0].graphic.layers.find((layer:{content:{kind:string}})=>layer.content.kind==='text')
  layer.content.parameters.font='Missing-Browser-Font-1502'
  nestedComposition.document=project
 }
</script>
<ProjectFonts />
<button onclick={showMissing}>Проверить отсутствующий шрифт</button>
<button onclick={undoNestedComposition} disabled={nestedHistory.busy||!nestedHistory.past.length}>Undo</button>
<button onclick={redoNestedComposition} disabled={nestedHistory.busy||!nestedHistory.future.length}>Redo</button>
<output aria-label="Project font history">{nestedHistory.past.length}</output>
<pre aria-label="Project document">{JSON.stringify(nestedComposition.document)}</pre>
<pre aria-label="Project renders">{JSON.stringify(nestedComposition.renderedOutputs)}</pre>
{#if nestedHistory.error}<p role="alert">{nestedHistory.error}</p>{/if}
<style>pre{max-width:100%;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}</style>
