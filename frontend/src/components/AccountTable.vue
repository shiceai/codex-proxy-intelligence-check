<script setup lang="ts">
import type { TableColumn } from '@codex-proxy/ui'
import type { Account } from '@/api'
import type { RowStatus } from '@/composables/useIntelligenceCheck'
import { ZCheckbox, ZTable } from '@codex-proxy/ui'
import { computed } from 'vue'
import { passRate } from '@/utils/jobs'
import ResultBar from './ResultBar.vue'

const props = defineProps<{
  accounts: Account[]
  selected: Set<string>
  rowStatus: Record<string, RowStatus>
  running: boolean
  loading: boolean
}>()

const emit = defineEmits<{
  toggle: [accountId: string, checked: boolean]
  toggleAll: [checked: boolean]
  open: [accountId: string, resultId: string]
}>()

const columns: TableColumn<Account>[] = [
  { key: 'select', kind: 'custom', size: 'xs', hideable: false },
  { key: 'label', label: '账号', kind: 'identity', hideable: false },
  { key: 'plan_type', label: '套餐', kind: 'meta', size: 'sm', emptyText: '--' },
  { key: 'recent', label: '最近 20 次', kind: 'custom', size: 'lg' },
  { key: 'rate', label: '通过率', kind: 'numeric', size: 'sm' },
  { key: 'last', label: '最近检测', kind: 'datetime', size: 'md' },
  { key: 'status', label: '状态', kind: 'status', size: 'lg' },
]

const allSelected = computed(() => props.accounts.length > 0 && props.selected.size === props.accounts.length)
const someSelected = computed(() => props.selected.size > 0 && !allSelected.value)

function rate(account: Account) {
  const value = passRate(account.recent.map(item => item.verdict))
  return value === null ? '--' : `${Math.round(value * 100)}%`
}

function lastCheck(account: Account) {
  const latest = account.recent[0]
  return latest ? new Date(latest.created_at_ms).toLocaleString('zh-CN') : '--'
}

function status(account: Account) {
  const value = props.rowStatus[account.id]
  if (value === 'queued')
    return '排队中'
  if (value === 'running')
    return '检测中'
  return value ? value.note : ''
}
</script>

<template>
  <ZTable :columns="columns" :data="accounts" row-key="id" :loading="loading" empty-text="没有已启用的 OpenAI 账号">
    <template #header-select>
      <ZCheckbox
        :model-value="allSelected"
        :indeterminate="someSelected"
        :disabled="running || accounts.length === 0"
        aria-label="全选账号"
        @update:model-value="emit('toggleAll', $event)"
      />
    </template>
    <template #select="{ row }">
      <ZCheckbox
        :model-value="selected.has(row.id)"
        :disabled="running"
        :aria-label="`选择 ${row.label}`"
        @update:model-value="emit('toggle', row.id, $event)"
      />
    </template>
    <template #recent="{ row }">
      <ResultBar :recent="row.recent" @open="emit('open', row.id, $event)" />
    </template>
    <template #rate="{ row }">
      {{ rate(row) }}
    </template>
    <template #last="{ row }">
      {{ lastCheck(row) }}
    </template>
    <template #status="{ row }">
      <span :class="typeof rowStatus[row.id] === 'object' ? 'text-z-error' : 'text-z-text-secondary'">
        {{ status(row) }}
      </span>
    </template>
  </ZTable>
</template>
