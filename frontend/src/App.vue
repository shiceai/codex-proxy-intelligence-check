<script setup lang="ts">
import { ZButton, ZConfigProvider } from '@codex-proxy/ui'
import { zhHans } from '@codex-proxy/ui/locale'
import { FlaskConical } from '@lucide/vue'
import { computed, onMounted, shallowRef } from 'vue'
import AccountTable from './components/AccountTable.vue'
import CheckToolbar from './components/CheckToolbar.vue'
import QuestionBankDialog from './components/QuestionBankDialog.vue'
import ResultDetailDialog from './components/ResultDetailDialog.vue'
import { useIntelligenceCheck } from './composables/useIntelligenceCheck'

defineProps<{
  preview: boolean
}>()

const check = useIntelligenceCheck()
const {
  loading,
  loadError,
  accounts,
  keys,
  bank,
  keyId,
  models,
  modelsLoading,
  model,
  effort,
  selected,
  running,
  stopping,
  waiting,
  progress,
  rowStatus,
  job,
} = check

const bankOpen = shallowRef(false)
const detail = shallowRef<{ accountId: string, resultId: string } | null>(null)
const detailUnsaved = computed(() => job.value?.tasks.some(({ result }) => result !== null && result.id === detail.value?.resultId && !result.saved) ?? false)
const detailOpen = computed({
  get: () => detail.value !== null,
  set: (value: boolean) => {
    if (!value)
      detail.value = null
  },
})

onMounted(check.reload)
</script>

<template>
  <ZConfigProvider :locale="zhHans">
    <main class="flex min-w-0 flex-col gap-4 font-sans text-z-text" :style="{ minHeight: preview ? '100dvh' : 'inherit' }">
      <div
        v-if="preview"
        class="flex shrink-0 items-center gap-2 rounded-z bg-z-warning-container px-3 py-2 text-z-sm text-z-warning-on-container"
        role="note"
      >
        <FlaskConical class="size-4 shrink-0" aria-hidden="true" />
        独立模拟预览，不代表宿主安装或真实检测结果
      </div>

      <div v-if="loadError" class="flex items-center gap-3 rounded-z bg-z-error-container px-3 py-2 text-z-sm text-z-error-on-container" role="alert">
        加载失败：{{ loadError }}
        <ZButton size="small" @click="check.reload">
          重试
        </ZButton>
      </div>

      <CheckToolbar
        v-model:key-id="keyId"
        v-model:model="model"
        v-model:effort="effort"
        :keys="keys"
        :models="models"
        :models-loading="modelsLoading"
        :running="running"
        :stopping="stopping"
        :waiting="waiting"
        :progress="progress"
        :selected-count="selected.size"
        @start="check.start"
        @stop="check.stop"
        @open-bank="bankOpen = true"
      />

      <AccountTable
        :accounts="accounts"
        :selected="selected"
        :row-status="rowStatus"
        :running="running"
        :loading="loading"
        @toggle="check.toggle"
        @toggle-all="check.toggleAll"
        @open="(accountId, resultId) => (detail = { accountId, resultId })"
      />

      <QuestionBankDialog v-model="bankOpen" :bank="bank" @saved="check.setBank" @reload="check.reload" />
      <ResultDetailDialog
        v-if="detail"
        v-model="detailOpen"
        :account-id="detail.accountId"
        :result-id="detail.resultId"
        :unsaved="detailUnsaved"
      />

      <p class="text-z-xs text-z-text-tertiary">
        检测请求经所选 Key 计量；账号必须在该 Key 的分组范围内。结果反映当前配置链路下该账号的表现。
      </p>
    </main>
  </ZConfigProvider>
</template>
