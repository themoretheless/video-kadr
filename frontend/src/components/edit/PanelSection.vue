<script lang="ts">
// Module scope: one shared counter gives every mounted section a unique pair of
// ids for aria-controls / aria-labelledby.
let sectionCount = 0
</script>

<script setup lang="ts">
import { computed, ref } from 'vue'

const props = withDefaults(
  defineProps<{
    title: string
    /** Optional short line under the title, e.g. a count of active items. */
    summary?: string
    open?: boolean
  }>(),
  { summary: '', open: false },
)

const uid = `panel-section-${(sectionCount += 1)}`
const headerId = `${uid}-header`
const bodyId = `${uid}-body`

const expanded = ref(props.open)

function toggle(): void {
  expanded.value = !expanded.value
}

const toggleLabel = computed(() => (expanded.value ? 'Свернуть' : 'Развернуть'))
</script>

<template>
  <section class="panel-section" :class="{ 'is-open': expanded }">
    <h3 class="panel-section-head">
      <button
        :id="headerId"
        type="button"
        class="panel-section-toggle"
        :aria-expanded="expanded"
        :aria-controls="bodyId"
        :title="toggleLabel"
        @click="toggle"
      >
        <span class="panel-section-chevron" aria-hidden="true">›</span>
        <span class="panel-section-title">{{ title }}</span>
        <span v-if="summary" class="panel-section-summary">{{ summary }}</span>
      </button>
    </h3>
    <div
      v-show="expanded"
      :id="bodyId"
      class="panel-section-body"
      role="region"
      :aria-labelledby="headerId"
    >
      <slot />
    </div>
  </section>
</template>

<style scoped>
.panel-section {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--panel-2);
  margin-bottom: 8px;
}

.panel-section-head {
  margin: 0;
  font-size: inherit;
  font-weight: inherit;
}

.panel-section-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 10px 12px;
  background: none;
  border: 0;
  border-radius: var(--radius);
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.panel-section-toggle:focus-visible {
  outline: none;
  box-shadow: var(--ring);
}

.panel-section-title {
  font-weight: 600;
}

.panel-section-summary {
  margin-left: auto;
  color: var(--muted);
  font-size: 12px;
}

.panel-section-chevron {
  display: inline-block;
  color: var(--faint);
  transition: transform 0.15s ease;
}

.is-open .panel-section-chevron {
  transform: rotate(90deg);
}

.panel-section-body {
  padding: 0 12px 12px;
  border-top: 1px solid var(--border);
  padding-top: 12px;
}

@media (prefers-reduced-motion: reduce) {
  .panel-section-chevron {
    transition: none;
  }
}
</style>
