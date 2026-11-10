<script setup lang="ts">
import type { MatchMode, Question, QuestionBank } from '@/api'
import { ZButton, ZDialog, ZInput, ZSelect, ZSwitch, ZTag, ZTextarea } from '@codex-proxy/ui'
import { computed, ref, watch } from 'vue'
import { saveQuestions } from '@/api'
import { errorMessage } from '@/utils/notification'

const props = defineProps<{
  bank: QuestionBank
}>()

const emit = defineEmits<{
  saved: [bank: QuestionBank]
  reload: []
}>()

const open = defineModel<boolean>({ default: false })

// 与后端校验一致：按字符计数，去掉首尾空白后再判断。
const MAX_QUESTIONS = 20
const limits = { name: 40, prompt: 2000, expected: 200 } as const
const matchOptions = [
  { label: '完全一致', value: 'exact' },
  { label: '包含', value: 'contains' },
]
const matchLabels: Record<MatchMode, string> = { exact: '完全一致', contains: '包含' }

const draft = ref<Question[]>([])
const editing = ref<Question | null>(null)
const editingIndex = ref<number | null>(null)
const saving = ref(false)
const saveError = ref<string | null>(null)

watch(open, (value) => {
  if (!value)
    return
  draft.value = props.bank.items.map(item => ({ ...item }))
  editing.value = null
  saveError.value = null
})

function length(text: string) {
  return [...text.trim()].length
}

const formProblem = computed(() => {
  const form = editing.value
  if (!form)
    return null
  if (length(form.name) < 1 || length(form.name) > limits.name)
    return `题目名称需为 1–${limits.name} 个字符`
  if (length(form.prompt) < 1 || length(form.prompt) > limits.prompt)
    return `题面需为 1–${limits.prompt} 个字符`
  if (length(form.expected) < 1 || length(form.expected) > limits.expected)
    return `标准答案需为 1–${limits.expected} 个字符`
  return null
})

const matchValue = computed({
  get: () => editing.value?.match,
  set: (value: string | undefined) => {
    if (editing.value && value)
      editing.value.match = value as MatchMode
  },
})

function add() {
  editingIndex.value = null
  editing.value = { id: '', name: '', prompt: '', expected: '', match: 'exact', enabled: true, builtin: false }
}

function edit(index: number) {
  const item = draft.value[index]
  if (!item)
    return
  editingIndex.value = index
  editing.value = { ...item }
}

function remove(index: number) {
  draft.value.splice(index, 1)
}

function confirmEdit() {
  const form = editing.value
  if (!form || formProblem.value)
    return
  const item = { ...form, name: form.name.trim(), prompt: form.prompt.trim(), expected: form.expected.trim() }
  if (editingIndex.value === null)
    draft.value.push(item)
  else
    draft.value[editingIndex.value] = item
  editing.value = null
}

async function save() {
  saving.value = true
  saveError.value = null
  try {
    emit('saved', await saveQuestions({ version: props.bank.version, items: draft.value }))
    open.value = false
  }
  catch (error) {
    saveError.value = errorMessage(error)
  }
  finally {
    saving.value = false
  }
}

function reload() {
  emit('reload')
  open.value = false
}
</script>

<template>
  <ZDialog v-model="open" title="题库" description="检测时依次运行所有已启用的题目；内置题只能启用或停用" width="48rem">
    <div class="flex flex-col gap-3">
      <ul class="flex flex-col gap-2">
        <li
          v-for="(item, index) in draft"
          :key="item.id || `new-${index}`"
          class="flex items-center gap-3 rounded-z bg-z-fill-quaternary px-3 py-2"
        >
          <ZSwitch v-model="item.enabled" :aria-label="`启用 ${item.name}`" />
          <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <div class="flex items-center gap-2">
              <span class="truncate font-emphasis">{{ item.name }}</span>
              <ZTag v-if="item.builtin" size="small" type="info">
                内置
              </ZTag>
            </div>
            <span class="text-z-xs text-z-text-tertiary">
              标准答案：{{ item.expected }} · {{ matchLabels[item.match] }}
            </span>
          </div>
          <template v-if="!item.builtin">
            <ZButton size="small" variant="text" :disabled="editing !== null" @click="edit(index)">
              编辑
            </ZButton>
            <ZButton size="small" variant="text" type="danger" :disabled="editing !== null" @click="remove(index)">
              删除
            </ZButton>
          </template>
        </li>
      </ul>

      <div v-if="editing" class="flex flex-col gap-3 rounded-z border border-z-border p-3">
        <div class="flex flex-col gap-1 text-z-sm text-z-text-secondary">
          <span>题目名称</span>
          <ZInput v-model="editing.name" placeholder="例如：数列推理" aria-label="题目名称" />
        </div>
        <div class="flex flex-col gap-1 text-z-sm text-z-text-secondary">
          <span>题面（作为唯一一条用户消息原样发送）</span>
          <ZTextarea v-model="editing.prompt" :rows="6" aria-label="题面" />
        </div>
        <div class="flex flex-wrap gap-3">
          <div class="flex min-w-48 flex-1 flex-col gap-1 text-z-sm text-z-text-secondary">
            <span>标准答案</span>
            <ZInput v-model="editing.expected" aria-label="标准答案" />
          </div>
          <div class="flex w-40 flex-col gap-1 text-z-sm text-z-text-secondary">
            <span>匹配方式</span>
            <ZSelect v-model="matchValue" :options="matchOptions" aria-label="匹配方式" />
          </div>
        </div>
        <div class="flex items-center justify-end gap-2">
          <span v-if="formProblem" class="mr-auto text-z-xs text-z-error">{{ formProblem }}</span>
          <ZButton size="small" @click="editing = null">
            取消
          </ZButton>
          <ZButton size="small" type="primary" :disabled="formProblem !== null" @click="confirmEdit">
            确定
          </ZButton>
        </div>
      </div>
      <ZButton v-else :disabled="draft.length >= MAX_QUESTIONS" @click="add">
        {{ draft.length >= MAX_QUESTIONS ? `题库最多 ${MAX_QUESTIONS} 道题` : '新增题目' }}
      </ZButton>
    </div>

    <template #footer>
      <div class="flex w-full items-center justify-end gap-2">
        <span v-if="saveError" class="mr-auto text-z-sm text-z-error" role="alert">{{ saveError }}</span>
        <ZButton v-if="saveError" @click="reload">
          重新加载
        </ZButton>
        <ZButton @click="open = false">
          取消
        </ZButton>
        <ZButton type="primary" :loading="saving" :disabled="editing !== null" @click="save">
          保存
        </ZButton>
      </div>
    </template>
  </ZDialog>
</template>
