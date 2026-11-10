<script setup lang="ts">
import type { ClientKey, Effort } from '@/api'
import { ZButton, ZSelect } from '@codex-proxy/ui'
import { computed } from 'vue'

const props = defineProps<{
  keys: ClientKey[]
  models: string[]
  modelsLoading: boolean
  running: boolean
  stopping: boolean
  waiting: boolean
  progress: { done: number, total: number }
  selectedCount: number
}>()

const emit = defineEmits<{
  start: []
  stop: []
  openBank: []
}>()

const keyId = defineModel<string | null>('keyId', { required: true })
const model = defineModel<string | null>('model', { required: true })
const effort = defineModel<Effort | null>('effort', { required: true })

const NO_EFFORT = 'none'
const keyOptions = computed(() => props.keys.map(key => ({ label: key.name, value: key.id })))
const modelOptions = computed(() => props.models.map(id => ({ label: id, value: id })))
const effortOptions = [
  { label: '不指定', value: NO_EFFORT },
  { label: 'low', value: 'low' },
  { label: 'medium', value: 'medium' },
  { label: 'high', value: 'high' },
  { label: 'xhigh', value: 'xhigh' },
]

// ZSelect 用 undefined 表示未选择，页面状态用 null。
const keyValue = computed({
  get: () => keyId.value ?? undefined,
  set: (value: string | undefined) => { keyId.value = value ?? null },
})
const modelValue = computed({
  get: () => model.value ?? undefined,
  set: (value: string | undefined) => { model.value = value ?? null },
})
const effortValue = computed({
  get: () => effort.value ?? NO_EFFORT,
  set: (value: string | undefined) => { effort.value = !value || value === NO_EFFORT ? null : value as Effort },
})
</script>

<template>
  <div class="flex flex-wrap items-end gap-3">
    <div class="flex w-56 flex-col gap-1 text-z-sm text-z-text-secondary">
      <span>计量 Key</span>
      <ZSelect v-model="keyValue" :options="keyOptions" :disabled="running" placeholder="选择 Key" empty-text="没有可用的 Key" aria-label="计量 Key" />
    </div>
    <div class="flex w-56 flex-col gap-1 text-z-sm text-z-text-secondary">
      <span>模型</span>
      <ZSelect
        v-model="modelValue"
        :options="modelOptions"
        :disabled="running || !keyId"
        :loading="modelsLoading"
        filterable
        placeholder="选择模型"
        empty-text="该 Key 没有可见模型"
        aria-label="模型"
      />
    </div>
    <div class="flex w-36 flex-col gap-1 text-z-sm text-z-text-secondary">
      <span>推理强度</span>
      <ZSelect v-model="effortValue" :options="effortOptions" :disabled="running" aria-label="推理强度" />
    </div>
    <div class="ml-auto flex items-center gap-2">
      <span v-if="running" class="text-z-sm text-z-text-secondary" aria-live="polite">
        {{ waiting ? '等待后台开始（最多约 30 秒）' : `后台检测中，已完成 ${progress.done} / ${progress.total}` }}
      </span>
      <ZButton :disabled="running" @click="emit('openBank')">
        题库
      </ZButton>
      <ZButton v-if="running" type="danger" variant="plain" :loading="stopping" @click="emit('stop')">
        {{ stopping ? '正在停止' : '停止' }}
      </ZButton>
      <ZButton v-else type="primary" @click="emit('start')">
        开始检测{{ selectedCount ? `（${selectedCount}）` : '' }}
      </ZButton>
    </div>
  </div>
</template>
