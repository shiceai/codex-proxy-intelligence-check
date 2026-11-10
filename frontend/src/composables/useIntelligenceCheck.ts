import type { Account, ClientKey, Effort, Job, QuestionBank } from '@/api'
import type { RowStatus } from '@/utils/jobs'
import { computed, onScopeDispose, ref, watch } from 'vue'
import { getBootstrap, getJob, listModels, startJob, stopJob } from '@/api'
import { isActive, progress as jobProgress, mergeRecent, rowStatuses, waitingToStart } from '@/utils/jobs'
import { pickModel } from '@/utils/models'
import { errorMessage, notification } from '@/utils/notification'

export type { RowStatus }

/** 后台执行期间查询进度的间隔。 */
const POLL_INTERVAL_MS = 3000

export function useIntelligenceCheck() {
  const loading = ref(true)
  const loadError = ref<string | null>(null)
  const accounts = ref<Account[]>([])
  const keys = ref<ClientKey[]>([])
  const bank = ref<QuestionBank>({ version: null, items: [] })

  const keyId = ref<string | null>(null)
  const models = ref<string[]>([])
  const modelsLoading = ref(false)
  const model = ref<string | null>(null)
  const effort = ref<Effort | null>('high')
  const selected = ref(new Set<string>())

  const job = ref<Job | null>(null)
  const submitting = ref(false)
  const running = computed(() => submitting.value || isActive(job.value))
  const stopping = computed(() => job.value?.status === 'stopping')
  const progress = computed(() => jobProgress(job.value))
  const waiting = computed(() => waitingToStart(job.value))
  const rowStatus = computed<Record<string, RowStatus>>(() => rowStatuses(job.value))

  let timer: ReturnType<typeof setTimeout> | null = null

  function schedulePoll() {
    if (timer)
      clearTimeout(timer)
    timer = isActive(job.value) ? setTimeout(poll, POLL_INTERVAL_MS) : null
  }

  function applyJob(next: Job | null) {
    job.value = next
    accounts.value = mergeRecent(accounts.value, next)
    schedulePoll()
  }

  async function poll() {
    timer = null
    try {
      applyJob(await getJob())
    }
    catch (error) {
      notification({ type: 'error', message: `检测进度查询失败：${errorMessage(error)}`, deduplicate: true })
      schedulePoll()
    }
  }

  onScopeDispose(() => {
    if (timer)
      clearTimeout(timer)
  })

  // 打开或刷新页面时，若后台仍有进行中的任务，自动恢复进度查询。
  async function reload() {
    loading.value = true
    loadError.value = null
    try {
      const data = await getBootstrap()
      accounts.value = data.accounts
      keys.value = data.keys
      bank.value = data.questions
      applyJob(data.job)
      const ids = new Set(data.accounts.map(account => account.id))
      selected.value = new Set([...selected.value].filter(id => ids.has(id)))
      if (!keyId.value || !data.keys.some(key => key.id === keyId.value))
        keyId.value = data.keys[0]?.id ?? null
    }
    catch (error) {
      loadError.value = errorMessage(error)
    }
    finally {
      loading.value = false
    }
  }

  watch(keyId, async (id) => {
    const previous = model.value
    models.value = []
    model.value = null
    if (!id)
      return
    modelsLoading.value = true
    try {
      const list = await listModels(id)
      if (keyId.value !== id)
        return
      models.value = list
      model.value = pickModel(list, previous)
    }
    catch (error) {
      notification({ type: 'error', message: `模型列表加载失败：${errorMessage(error)}` })
    }
    finally {
      modelsLoading.value = false
    }
  })

  function toggle(id: string, checked: boolean) {
    const next = new Set(selected.value)
    if (checked)
      next.add(id)
    else
      next.delete(id)
    selected.value = next
  }

  function toggleAll(checked: boolean) {
    selected.value = new Set(checked ? accounts.value.map(account => account.id) : [])
  }

  function setBank(next: QuestionBank) {
    bank.value = next
  }

  function startProblem(): string | null {
    if (!keyId.value)
      return '请选择用于计量的 Key'
    if (!model.value)
      return '请选择模型'
    if (selected.value.size === 0)
      return '请至少勾选一个账号'
    if (!bank.value.items.some(question => question.enabled))
      return '题库中没有启用的题目'
    return null
  }

  async function start() {
    const problem = startProblem()
    if (problem) {
      notification({ type: 'warning', message: problem })
      return
    }
    submitting.value = true
    try {
      applyJob(await startJob({
        account_ids: accounts.value.filter(account => selected.value.has(account.id)).map(account => account.id),
        key_id: keyId.value as string,
        model: model.value as string,
        effort: effort.value,
      }))
    }
    catch (error) {
      notification({ type: 'error', message: errorMessage(error) })
    }
    finally {
      submitting.value = false
    }
  }

  async function stop() {
    if (!job.value || !isActive(job.value))
      return
    try {
      applyJob(await stopJob(job.value.id))
    }
    catch (error) {
      notification({ type: 'error', message: errorMessage(error) })
    }
  }

  return {
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
    job,
    running,
    stopping,
    waiting,
    progress,
    rowStatus,
    reload,
    toggle,
    toggleAll,
    setBank,
    start,
    stop,
  }
}
