import type { Bootstrap, CheckResult, Job, JobInput, QuestionBank } from './types'
import request from './request'

export type * from './host'
export type * from './types'

export function getBootstrap(): Promise<Bootstrap> {
  return request('GET', 'api/bootstrap')
}

export async function listModels(keyId: string): Promise<string[]> {
  const reply = await request<{ models: string[] }>('POST', 'api/models', { key_id: keyId })
  return reply.models
}

export async function listResults(accountId: string): Promise<CheckResult[]> {
  const reply = await request<{ items: CheckResult[] }>('POST', 'api/results', { account_id: accountId })
  return reply.items
}

export function saveQuestions(bank: QuestionBank): Promise<QuestionBank> {
  return request('POST', 'api/questions', bank)
}

export async function getJob(): Promise<Job | null> {
  const reply = await request<{ job: Job | null }>('GET', 'api/jobs/current')
  return reply.job
}

/** 提交一批检测，由插件后台执行；离开或刷新页面不影响。 */
export async function startJob(input: JobInput): Promise<Job> {
  const reply = await request<{ job: Job }>('POST', 'api/jobs', input)
  return reply.job
}

export async function stopJob(jobId: string): Promise<Job> {
  const reply = await request<{ job: Job }>('POST', 'api/jobs/stop', { job_id: jobId })
  return reply.job
}
