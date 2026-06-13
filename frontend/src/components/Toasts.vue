<script setup lang="ts">
import { toasts, dismissToast } from '../toasts'
</script>

<template>
  <div class="toasts" aria-live="polite">
    <transition-group name="toast">
      <div
        v-for="t in toasts"
        :key="t.id"
        class="toast"
        :class="t.kind"
        role="status"
        @click="dismissToast(t.id)"
      >
        <span class="toast-dot"></span>
        <span class="toast-text">{{ t.text }}</span>
      </div>
    </transition-group>
  </div>
</template>
