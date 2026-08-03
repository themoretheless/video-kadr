<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import {
  acceptProjectRecovery,
  discardAutosaveRecovery,
  leaveUnrecoverableProject,
  projectRecovery,
} from '../store'

const primary = ref<HTMLButtonElement | null>(null)
const dialog = ref<HTMLElement | null>(null)
let previousFocus: HTMLElement | null = null
const recovery = computed(() => projectRecovery.candidate)
const title = computed(() => recovery.value?.candidate?.name || 'Проект')
const autosaveTime = computed(() => {
  const value = recovery.value?.candidate?.updatedAt
  return value ? new Date(value).toLocaleString() : 'неизвестно'
})

watch(recovery, async (value) => {
  if (!value) {
    previousFocus?.focus()
    previousFocus = null
    return
  }
  previousFocus = document.activeElement as HTMLElement | null
  await nextTick()
  if (primary.value) primary.value.focus()
  else dialog.value?.focus()
})

function trapKeyboard(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    return
  }
  if (event.key !== 'Tab') return
  const buttons = [...(dialog.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])]
  if (!buttons.length) return
  const current = buttons.indexOf(document.activeElement as HTMLButtonElement)
  const next = event.shiftKey
    ? (current <= 0 ? buttons.length - 1 : current - 1)
    : (current < 0 || current === buttons.length - 1 ? 0 : current + 1)
  event.preventDefault()
  buttons[next]?.focus()
}
</script>

<template>
  <div v-if="recovery" class="recovery-backdrop">
    <section
      ref="dialog"
      class="recovery-dialog card"
      role="alertdialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="recovery-title"
      aria-describedby="recovery-description"
      @keydown="trapKeyboard"
    >
      <h2 id="recovery-title">
        {{ recovery.reason === 'draft' ? 'Восстановить несохранённые изменения?' : 'Последнее сохранение повреждено' }}
      </h2>
      <p id="recovery-description">
        Проект «{{ title }}» · автосохранение {{ autosaveTime }}.
        <template v-if="recovery.reason === 'draft'">
          Найдена целая локальная версия после неожиданного завершения предыдущего сеанса.
        </template>
        <template v-else-if="recovery.candidate">
          Можно безопасно восстановить последнюю проверенную ревизию. Повреждённая запись не будет перезаписана молча.
        </template>
        <template v-else>
          Исправной резервной ревизии не найдено. Автосохранение заблокировано.
        </template>
      </p>
      <p v-if="projectRecovery.error" class="error" role="alert">{{ projectRecovery.error }}</p>
      <div class="recovery-actions">
        <button
          v-if="recovery.candidate"
          ref="primary"
          class="btn primary"
          :disabled="projectRecovery.busy"
          @click="acceptProjectRecovery"
        >{{ recovery.reason === 'draft' ? 'Восстановить' : 'Откатить к исправной ревизии' }}</button>
        <button
          v-if="recovery.reason === 'draft'"
          class="btn ghost"
          :disabled="projectRecovery.busy"
          @click="discardAutosaveRecovery"
        >Открыть сохранённую</button>
        <button
          v-if="!recovery.candidate"
          ref="primary"
          class="btn ghost"
          :disabled="projectRecovery.busy"
          @click="leaveUnrecoverableProject"
        >Вернуться в медиатеку</button>
      </div>
    </section>
  </div>
</template>
