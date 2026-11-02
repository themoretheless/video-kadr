<script lang="ts">
  import CompositionInspector from '../../src/lib/components/composition/CompositionInspector.svelte'
  import CompositionPreview from '../../src/lib/components/composition/CompositionPreview.svelte'
  import { compositionState, undoComposition, redoComposition } from '../../src/lib/state/composition.svelte'
</script>
<div class="layout"><CompositionPreview /><CompositionInspector /></div>
<button onclick={undoComposition}>Undo</button><button onclick={redoComposition}>Redo</button>
<output aria-label="Graphic track history">{compositionState.history.past.length}</output>
<pre aria-label="Graphic track document">{JSON.stringify(compositionState.document)}</pre>
<style>.layout { display: grid; grid-template-columns: minmax(240px, 1fr) minmax(280px, 420px); gap: 16px; } pre { max-height: 140px; overflow: auto; font-size: 10px; }</style>
