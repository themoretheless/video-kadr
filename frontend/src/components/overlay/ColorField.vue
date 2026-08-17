<script setup lang="ts">
// A labelled colour swatch. The wire only accepts `#RRGGBB`, so the native
// picker (which always produces that shape) is the source of truth and a typed
// value is only accepted once it matches.

import { computed } from 'vue'

const props = defineProps<{ modelValue: string; label: string }>()
const emit = defineEmits<{ 'update:modelValue': [string] }>()

const HEX = /^#[0-9a-fA-F]{6}$/

const value = computed(() => (HEX.test(props.modelValue) ? props.modelValue : '#000000'))

function commit(raw: string): void {
  const candidate = raw.trim()
  if (HEX.test(candidate)) emit('update:modelValue', candidate.toUpperCase())
}
</script>

<template>
  <label class="ovl-color">
    <span>{{ label }}</span>
    <input
      type="color"
      :value="value"
      :aria-label="label"
      @input="commit(($event.target as HTMLInputElement).value)"
    />
    <input
      class="time-input ovl-color-hex"
      :value="value"
      :aria-label="`${label}, HEX`"
      maxlength="7"
      @change="commit(($event.target as HTMLInputElement).value)"
    />
    <slot />
  </label>
</template>

<style scoped>
.ovl-color {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--muted);
}

.ovl-color input[type='color'] {
  width: 36px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  cursor: pointer;
}

.ovl-color-hex {
  width: 92px;
  text-transform: uppercase;
}
</style>
