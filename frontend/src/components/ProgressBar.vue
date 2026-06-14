<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  progress: number | null
  stage?: string | null
  cancellable?: boolean
}>()

defineEmits<{ cancel: [] }>()

const stageLabels: Record<string, string> = {
  queued: 'В очереди',
  uploading: 'Загружаю',
  downloading: 'Скачиваю',
  processing: 'Обрабатываю',
}

const label = computed(() => {
  const s = props.stage ? stageLabels[props.stage] ?? props.stage : 'Работаю'
  if (typeof props.progress === 'number') return `${s} · ${Math.round(props.progress)}%`
  return `${s}…`
})

const known = computed(() => typeof props.progress === 'number')
</script>

<template>
  <div class="progress">
    <div class="progress-head">
      <span class="progress-label">{{ label }}</span>
      <button v-if="cancellable" type="button" class="btn ghost sm" @click="$emit('cancel')">
        Отмена
      </button>
    </div>
    <div class="progress-track">
      <div
        class="progress-fill"
        :class="{ indeterminate: !known }"
        :style="known ? { width: Math.max(2, progress as number) + '%' } : undefined"
      ></div>
    </div>
  </div>
</template>
