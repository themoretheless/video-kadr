<script lang="ts">
  import AudioDspChain from '../../src/lib/components/composition/AudioDspChain.svelte'
  import type { CompositionDspEffect } from '../../src/lib/composition/types'
  let effects = $state<readonly CompositionDspEffect[]>([])
  let localTick = $state(500000)
</script>
<label>Курсор<input aria-label="DSP cursor" type="number" bind:value={localTick} /></label>
<AudioDspChain {effects} {localTick} onchange={value => effects = value} />
<output aria-label="Authored DSP chain">{JSON.stringify(effects)}</output>
