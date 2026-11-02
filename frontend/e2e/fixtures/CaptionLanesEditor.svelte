<script lang="ts">
  import {nestedComposition,nestedHistory,undoNestedComposition} from '../../src/lib/state/nestedComposition.svelte'
  import NestedClipTransfer from '../../src/lib/components/composition/NestedClipTransfer.svelte'
  import NestedCaptionExport from '../../src/lib/components/composition/NestedCaptionExport.svelte'
  import AgentPanel from '../../src/lib/agent/AgentPanel.svelte'
  import Tools from '../../src/lib/components/composition/CompositionLocalTools.svelte'
  import Preview from '../../src/lib/components/composition/CompositionPreview.svelte'
  import { compositionState, selectCompositionClip, undoComposition, redoComposition, saveCompositionProject, openCompositionProject } from '../../src/lib/state/composition.svelte'
  import { buildCompositionWireDocument } from '../../src/lib/composition/payload'
  function openNested() {
    const content = JSON.parse(JSON.stringify(buildCompositionWireDocument(compositionState.document))) as ReturnType<typeof buildCompositionWireDocument>
    const compositions = Object.fromEntries(['root','child','unused'].map(id => [id,{durationUs:5_000_000,content:{...structuredClone(content),tracks:content.tracks.map(track=>({...track,clips:track.clips.map(clip=>({...clip,id:`${id}-${clip.id}`,...(track.kind==='text'?{text:`${id}: ${'text' in clip ? clip.text : ''}`}:{})}))})) as typeof content.tracks},instances:[]}]))
    nestedComposition.document = {schemaVersion:1,root:'root',compositions}
    nestedComposition.activeCompositionId = 'root'
    nestedComposition.renderedOutputs = {root:'obsolete',unused:'obsolete'}
  }
  async function save() {
    await saveCompositionProject()
    if (compositionState.projectId) localStorage.setItem('caption-lanes-saved-id', compositionState.projectId)
  }
  async function reopen() {
    const id = localStorage.getItem('caption-lanes-saved-id')
    if (id) await openCompositionProject(id)
  }
</script>
<AgentPanel />
{#if nestedComposition.document}<NestedClipTransfer /><NestedCaptionExport /><button onclick={() => {nestedComposition.activeCompositionId='unused'}}>Show unused captions</button>{/if}
<Preview />
<Tools />
<button onclick={openNested}>Open nested captions</button>
<button onclick={undoNestedComposition}>Undo nested captions</button>
<pre aria-label="Nested caption document">{JSON.stringify(nestedComposition.document)}</pre>
<output aria-label="Nested caption history">{nestedHistory.past.length}</output>
<output aria-label="Nested caption error">{nestedHistory.error}</output>
<button onclick={() => selectCompositionClip(null,null)}>Clear selection</button>
{#each compositionState.document.tracks as track (track.id)}
  {#if track.kind === 'text'}
    {#each track.clips as clip (clip.id)}
      <button onclick={() => selectCompositionClip(track.id, clip.id, {toggle:true})}>Select cue {clip.text}</button>
    {/each}
  {/if}
{/each}
<button onclick={() => undoComposition()}>Undo</button>
<button onclick={() => redoComposition()}>Redo</button>
<button onclick={() => void save()}>Save project</button>
<button onclick={() => void reopen()}>Reopen project</button>
<pre aria-label="Caption document">{JSON.stringify(compositionState.document)}</pre>
<pre aria-label="Caption wire document">{JSON.stringify(buildCompositionWireDocument(compositionState.document))}</pre>
<output aria-label="Caption save error">{compositionState.save.error}</output>
