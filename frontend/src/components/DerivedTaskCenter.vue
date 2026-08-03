<script setup lang="ts">
import { derivedTaskState, cancelDerivedTask, resumeDerivedPermission, retryDerivedTask, setDerivedTaskPriority } from '../derived-task-center'
import type { DerivedTask, DerivedTaskState } from '../browser-derived-queue'

const labels: Record<DerivedTaskState, string> = {
  blocked: 'Ждёт зависимость', queued: 'В очереди', running: 'Выполняется', retry_wait: 'Повтор позже',
  succeeded: 'Готово', failed: 'Ошибка', cancelled: 'Отменено', permission_required: 'Нужно разрешение',
}
const waiting = (task: DerivedTask) => ['blocked', 'queued', 'retry_wait', 'failed', 'permission_required'].includes(task.state)
</script>

<template>
  <section v-if="derivedTaskState.tasks.length || derivedTaskState.restoring" class="derived-task-center" aria-labelledby="derived-task-title">
    <div class="derived-task-heading">
      <div>
        <h2 id="derived-task-title">Фоновые задачи</h2>
        <p>{{ derivedTaskState.message }}</p>
      </div>
      <span v-if="derivedTaskState.restoring" role="status">Восстановление…</span>
    </div>
    <ul>
      <li v-for="task in derivedTaskState.tasks" :key="task.id">
        <div>
          <strong>{{ task.kind }}</strong>
          <span :class="`task-state ${task.state}`">{{ labels[task.state] }}</span>
          <small v-if="task.dependencies.length">зависимостей: {{ task.dependencies.length }}</small>
          <small v-if="task.error">{{ task.error }}</small>
        </div>
        <div class="derived-task-actions">
          <label v-if="waiting(task) && !['cancelled', 'succeeded'].includes(task.state)">
            Приоритет
            <select :value="task.priority" @change="setDerivedTaskPriority(task, Number(($event.target as HTMLSelectElement).value))">
              <option :value="-10">Фоновый</option><option :value="0">Обычный</option><option :value="10">Сейчас</option>
            </select>
          </label>
          <button v-if="task.state === 'failed'" class="btn ghost sm" @click="retryDerivedTask(task.id)">Повторить</button>
          <button v-if="task.state === 'permission_required'" class="btn primary sm" @click="resumeDerivedPermission(task)">Разрешить и продолжить</button>
          <button v-if="!['succeeded', 'failed', 'cancelled'].includes(task.state)" class="btn ghost sm" @click="cancelDerivedTask(task.id)">Отменить</button>
        </div>
      </li>
    </ul>
  </section>
</template>
