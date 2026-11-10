<script setup lang="ts">
import type { CheckResult, Verdict } from '@/api'
import { ZDialog, ZTag } from '@codex-proxy/ui'
import { computed, ref, watch } from 'vue'
import { listResults } from '@/api'
import { errorMessage } from '@/utils/notification'

const props = defineProps<{
  accountId: string
  resultId: string
  /** 后台任务记录显示该结果写入失败、未保存。 */
  unsaved: boolean
}>()

const open = defineModel<boolean>({ default: false })

const verdicts: Record<Verdict, { label: string, type: 'success' | 'danger' | 'default' }> = {
  normal: { label: '正常', type: 'success' },
  failed: { label: '未通过', type: 'danger' },
  error: { label: '错误', type: 'default' },
}

const result = ref<CheckResult | null>(null)
const loading = ref(false)
const problem = ref<string | null>(null)

watch([open, () => props.resultId], async ([value]) => {
  if (!value)
    return
  loading.value = true
  problem.value = null
  result.value = null
  try {
    const items = await listResults(props.accountId)
    result.value = items.find(item => item.id === props.resultId) ?? null
    if (!result.value)
      problem.value = props.unsaved ? '该结果写入失败，未保存，无法查看详情' : '该结果已不在最近 20 条记录中'
  }
  catch (error) {
    problem.value = `加载失败：${errorMessage(error)}`
  }
  finally {
    loading.value = false
  }
}, { immediate: true })

function text(value: string | number | null | undefined) {
  return value === null || value === undefined || value === '' ? '--' : String(value)
}

const fields = computed(() => {
  const item = result.value
  if (!item)
    return []
  return [
    ['账号', item.account_label],
    ['Key', item.key_name],
    ['请求模型', item.model],
    ['实际响应模型', text(item.response_model)],
    ['推理强度', item.effort ?? '不指定'],
    ['题目', item.question_name],
    ['标准答案', item.expected],
    ['匹配方式', item.match === 'exact' ? '完全一致' : '包含'],
    ['耗时', `${(item.duration_ms / 1000).toFixed(1)} 秒`],
    ['输入 / 输出 / 推理 token', `${text(item.input_tokens)} / ${text(item.output_tokens)} / ${text(item.reasoning_tokens)}`],
    ['请求 ID', text(item.request_id)],
    ['时间', new Date(item.created_at_ms).toLocaleString('zh-CN')],
  ] as const
})
</script>

<template>
  <ZDialog v-model="open" title="检测详情" width="40rem">
    <p v-if="loading" class="text-z-sm text-z-text-secondary">
      加载中
    </p>
    <p v-else-if="problem" class="text-z-sm text-z-text-secondary">
      {{ problem }}
    </p>
    <div v-else-if="result" class="flex flex-col gap-3 text-z-sm">
      <div class="flex items-center gap-2">
        <span class="text-z-text-secondary">结果</span>
        <ZTag :type="verdicts[result.verdict].type">
          {{ verdicts[result.verdict].label }}
        </ZTag>
      </div>
      <dl class="grid grid-cols-[8rem_minmax(0,1fr)] gap-x-3 gap-y-1.5">
        <template v-for="[label, value] in fields" :key="label">
          <dt class="text-z-text-secondary">
            {{ label }}
          </dt>
          <dd class="m-0 break-all">
            {{ value }}
          </dd>
        </template>
      </dl>
      <div class="flex flex-col gap-1">
        <span class="text-z-text-secondary">实际答案摘录</span>
        <pre class="m-0 max-h-48 overflow-auto rounded-z bg-z-fill-quaternary p-2 font-mono text-z-xs whitespace-pre-wrap">{{ result.answer_excerpt || '（无）' }}</pre>
      </div>
      <div v-if="result.error" class="flex flex-col gap-1">
        <span class="text-z-text-secondary">错误原因</span>
        <pre class="m-0 rounded-z bg-z-error-container p-2 font-mono text-z-xs whitespace-pre-wrap text-z-error-on-container">{{ result.error }}</pre>
      </div>
    </div>
  </ZDialog>
</template>
