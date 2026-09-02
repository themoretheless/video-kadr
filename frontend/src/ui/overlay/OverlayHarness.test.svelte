<script lang="ts">
  import Overlay from './Overlay.svelte'
  let anchor = $state<HTMLButtonElement>()
  let open = $state(false)
</script>

<button bind:this={anchor} onclick={() => { open = true }}>Открыть</button>
<Overlay {anchor} {open} onclose={() => { open = false }}>
  <span>Содержимое</span>
</Overlay>
