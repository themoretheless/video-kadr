<script lang="ts">
  import { dismissToast, toasts } from '$lib/state/toasts.svelte.js'
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as toast (toast.id)}
    <div class={`toast ${toast.kind}`} role="status">
      <span class="toast-dot"></span><span class="toast-text">{toast.text}</span>
      <button class="toast-dismiss" aria-label="Закрыть уведомление" onclick={() => dismissToast(toast.id)}>×</button>
    </div>
  {/each}
</div>
