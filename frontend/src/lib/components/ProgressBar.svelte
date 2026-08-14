<script lang="ts">
  interface Props {
    progress: number | null
    stage?: string | null
    cancellable?: boolean
    oncancel?: () => void
    class?: string
  }

  let { progress, stage = null, cancellable = false, oncancel, class: className = '' }: Props = $props()
  const stageLabels: Record<string, string> = {
    queued: 'В очереди', uploading: 'Загружаю', downloading: 'Скачиваю', processing: 'Обрабатываю',
  }
  let known = $derived(typeof progress === 'number')
  let label = $derived(`${stage ? stageLabels[stage] ?? stage : 'Работаю'}${known ? ` · ${Math.round(progress as number)}%` : '…'}`)
</script>

<div class={`progress ${className}`.trim()}>
  <div class="progress-head">
    <span class="progress-label">{label}</span>
    {#if cancellable}<button type="button" class="btn ghost sm" onclick={oncancel}>Отмена</button>{/if}
  </div>
  <div class="progress-track">
    <div
      class:indeterminate={!known}
      class="progress-fill"
      style:width={known ? `${Math.max(2, progress as number)}%` : undefined}
    ></div>
  </div>
</div>
