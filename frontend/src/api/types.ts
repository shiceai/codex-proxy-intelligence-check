export type Verdict = 'normal' | 'failed' | 'error'
export type Effort = 'low' | 'medium' | 'high' | 'xhigh'
export type MatchMode = 'exact' | 'contains'

export interface Question {
  id: string
  name: string
  prompt: string
  expected: string
  match: MatchMode
  enabled: boolean
  builtin: boolean
}

export interface QuestionBank {
  version: number | null
  items: Question[]
}

export interface Recent {
  id: string
  verdict: Verdict
  created_at_ms: number
}

export interface Account {
  id: string
  label: string
  plan_type: string | null
  recent: Recent[]
}

export interface ClientKey {
  id: string
  name: string
}

export interface Bootstrap {
  accounts: Account[]
  keys: ClientKey[]
  questions: QuestionBank
  job: Job | null
}

export interface CheckResult {
  id: string
  created_at_ms: number
  account_id: string
  account_label: string
  key_id: string
  key_name: string
  model: string
  response_model: string | null
  effort: Effort | null
  question_id: string
  question_name: string
  expected: string
  match: MatchMode
  verdict: Verdict
  answer_excerpt: string
  error: string | null
  duration_ms: number
  input_tokens: number | null
  output_tokens: number | null
  reasoning_tokens: number | null
  request_id: string | null
}

export type JobStatus = 'running' | 'stopping' | 'finished'
export type TaskState = 'pending' | 'running' | 'done' | 'skipped' | 'interrupted'

export interface TaskResult {
  id: string
  verdict: Verdict
  created_at_ms: number
  saved: boolean
}

export interface JobTask {
  account_id: string
  question_id: string
  state: TaskState
  result: TaskResult | null
  note: string | null
}

export interface Job {
  id: string
  created_at_ms: number
  finished_at_ms: number | null
  key_id: string
  key_name: string
  model: string
  effort: Effort | null
  status: JobStatus
  tasks: JobTask[]
}

export interface JobInput {
  account_ids: string[]
  key_id: string
  model: string
  effort: Effort | null
}
