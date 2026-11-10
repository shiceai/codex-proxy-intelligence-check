// @env browser
import type {
  Account,
  CheckResult,
  ClientKey,
  Job,
  JobInput,
  ManagementReply,
  ManagementRequestInput,
  PluginHost,
  Question,
  QuestionBank,
  Verdict,
} from '../api'
import { applyTheme, createTheme } from '@codex-proxy/ui/theme'

// 仅供 `pnpm dev` 独立预览：模拟宿主桥、插件接口与后台执行，生产构建不会安装。
const encoder = new TextEncoder()
const candy: Question = {
  id: 'candy',
  name: '糖果配对',
  prompt: '（预览数据）糖果配对题面',
  expected: '21',
  match: 'exact',
  enabled: true,
  builtin: true,
}
const accounts: Omit<Account, 'recent'>[] = [
  { id: 'acct_1', label: 'alice@example.com', plan_type: 'plus' },
  { id: 'acct_2', label: 'bob@example.com', plan_type: 'pro' },
  { id: 'acct_3', label: '团队账号', plan_type: null },
]
const keys: ClientKey[] = [
  { id: 'key_1', name: '检测专用 Key' },
  { id: 'key_2', name: '默认 Key' },
]
const results = new Map<string, CheckResult[]>()
let bank: QuestionBank = { version: null, items: [candy] }
let job: Job | null = null
let sequence = 0
const STORAGE_KEY = 'intelligence-check-preview'

// 模拟"插件后台"：状态存入 sessionStorage，刷新页面后任务继续推进。
function persist() {
  try {
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify({ bank, job, results: [...results] }))
  }
  catch {}
}

function restore() {
  try {
    const saved = sessionStorage.getItem(STORAGE_KEY)
    if (!saved)
      return
    const value = JSON.parse(saved) as { bank: QuestionBank, job: Job | null, results: [string, CheckResult[]][] }
    bank = value.bank
    job = value.job
    for (const [id, items] of value.results)
      results.set(id, items)
  }
  catch {}
}

function reply(value: unknown, status = 200): Promise<ManagementReply> {
  const bytes = encoder.encode(JSON.stringify(value))
  return Promise.resolve({ status, contentType: 'application/json', body: bytes.buffer })
}

function error(status: number, code: string, message: string) {
  return reply({ error: { code, message } }, status)
}

function finishTask(current: Job, index: number) {
  const task = current.tasks[index]
  if (!task)
    return
  const verdicts: Verdict[] = ['normal', 'normal', 'normal', 'failed', 'error']
  const verdict = verdicts[Math.floor(Math.random() * verdicts.length)] ?? 'normal'
  const question = bank.items.find(item => item.id === task.question_id) ?? candy
  sequence += 1
  const result: CheckResult = {
    id: `r${Date.now()}_${sequence}`,
    created_at_ms: Date.now(),
    account_id: task.account_id,
    account_label: accounts.find(item => item.id === task.account_id)?.label ?? task.account_id,
    key_id: current.key_id,
    key_name: current.key_name,
    model: current.model,
    response_model: verdict === 'error' ? null : current.model,
    effort: current.effort,
    question_id: question.id,
    question_name: question.name,
    expected: question.expected,
    match: question.match,
    verdict,
    answer_excerpt: verdict === 'normal' ? question.expected : verdict === 'failed' ? '29' : '',
    error: verdict === 'error' ? '响应未完成：max_output_tokens' : null,
    duration_ms: 42_000,
    input_tokens: 180,
    output_tokens: 900,
    reasoning_tokens: 860,
    request_id: verdict === 'error' ? null : `req_${sequence}`,
  }
  results.set(task.account_id, [result, ...(results.get(task.account_id) ?? [])].slice(0, 20))
  task.state = 'done'
  task.result = { id: result.id, verdict, created_at_ms: result.created_at_ms, saved: true }
}

// 每秒推进一次：完成进行中的题，再按"最多 3 个、同账号串行"开始新题。
function advance() {
  const current = job
  if (!current || current.status === 'finished')
    return
  current.tasks.forEach((task, index) => {
    if (task.state === 'running')
      finishTask(current, index)
  })
  if (current.status === 'stopping') {
    for (const task of current.tasks) {
      if (task.state === 'pending') {
        task.state = 'skipped'
        task.note = '已停止'
      }
    }
  }
  const busy = new Set<string>()
  for (const task of current.tasks) {
    if (busy.size >= 3)
      break
    if (task.state === 'pending' && !busy.has(task.account_id)) {
      task.state = 'running'
      busy.add(task.account_id)
    }
  }
  if (current.tasks.every(task => task.state !== 'pending' && task.state !== 'running')) {
    current.status = 'finished'
    current.finished_at_ms = Date.now()
  }
  persist()
}

function submitJob(input: JobInput): Promise<ManagementReply> {
  if (job && job.status !== 'finished')
    return error(409, 'job_active', '已有检测在进行，请等待完成或先停止')
  const questions = bank.items.filter(item => item.enabled)
  if (questions.length === 0)
    return error(400, 'invalid_request', '题库中没有启用的题目')
  const key = keys.find(item => item.id === input.key_id)
  job = {
    id: `j${Date.now()}`,
    created_at_ms: Date.now(),
    finished_at_ms: null,
    key_id: input.key_id,
    key_name: key?.name ?? input.key_id,
    model: input.model,
    effort: input.effort,
    status: 'running',
    tasks: input.account_ids.flatMap(accountId => questions.map(question => ({
      account_id: accountId,
      question_id: question.id,
      state: 'pending' as const,
      result: null,
      note: null,
    }))),
  }
  persist()
  return reply({ job })
}

function stopCurrent(jobId: string): Promise<ManagementReply> {
  if (!job || job.id !== jobId)
    return error(409, 'job_changed', '检测任务已变化，请刷新页面')
  if (job.status === 'running')
    job.status = 'stopping'
  persist()
  return reply({ job })
}

function saveBank(submitted: QuestionBank): Promise<ManagementReply> {
  if (submitted.version !== bank.version)
    return reply({ error: { code: 'conflict', message: '题库已被修改，请刷新后重试' } }, 409)
  bank = {
    version: (bank.version ?? 0) + 1,
    items: submitted.items.map((item, index) => item.id ? item : { ...item, id: `q_${Date.now()}_${index}` }),
  }
  persist()
  return reply(bank)
}

function managementRequest(input: ManagementRequestInput): Promise<ManagementReply> {
  const data: unknown = input.body ? JSON.parse(input.body) : undefined
  switch (`${input.method} ${input.path}`) {
    case 'GET api/bootstrap':
      return reply({
        accounts: accounts.map(account => ({
          ...account,
          recent: (results.get(account.id) ?? []).map(({ id, verdict, created_at_ms }) => ({ id, verdict, created_at_ms })),
        })),
        keys,
        questions: bank,
        job,
      })
    case 'POST api/models':
      return reply({ models: ['gpt-5.5', 'gpt-6-astra', 'gpt-5.4'] })
    case 'POST api/results':
      return reply({ items: results.get((data as { account_id: string }).account_id) ?? [] })
    case 'POST api/questions':
      return saveBank(data as QuestionBank)
    case 'GET api/jobs/current':
      return reply({ job })
    case 'POST api/jobs':
      return submitJob(data as JobInput)
    case 'POST api/jobs/stop':
      return stopCurrent((data as { job_id: string }).job_id)
    default:
      return reply({ error: { code: 'not_found', message: '未找到插件管理接口' } }, 404)
  }
}

export function installPreviewHost(): boolean {
  if (!import.meta.env.DEV || window.codexProxyPlugin)
    return false

  const parameters = new URLSearchParams(window.location.search)
  const theme = parameters.get('theme') === 'dark' ? 'dark' : 'light'
  applyTheme(document.documentElement, createTheme({ scheme: theme }))
  restore()
  setInterval(advance, 1000)
  const value: PluginHost = { version: 2, theme, request: managementRequest }
  Object.defineProperty(window, 'codexProxyPlugin', { configurable: true, value })
  return true
}
