<script lang="ts">
  let open = $state(false)
</script>
<details class="composition-tool-section" ontoggle={(event) => { open = event.currentTarget.open }}>
  <summary>Сцены и качество исходника</summary>
  {#if open}
    {#await import('./ShotReviewPanel.svelte')}<p role="status">Открываю просмотр сцен…</p>
    {:then panel}<panel.default />
    {:catch}<p class="error" role="alert">Просмотр сцен недоступен</p>{/await}
  {/if}
</details>
