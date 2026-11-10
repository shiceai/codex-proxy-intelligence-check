// 后台检测任务的展示推导：不依赖任何运行时模块，便于 node:test 直接运行。
import type { Account, Job, Verdict } from '../api/types'

export type RowStatus = 'queued' | 'running' | { note: string }

const KEEP = 20

export function isActive(job: Job | null): boolean {
  return job !== null && job.status !== 'finished'
}

export function progress(job: Job | null): { done: number, total: number } {
  if (!job)
    return { done: 0, total: 0 }
  const done = job.tasks.filter(task => task.state !== 'pending' && task.state !== 'running').length
  return { done, total: job.tasks.length }
}

/** 所有题都还在排队：新任务要等宿主的下一轮后台调用才开始（最多约 30 秒）。 */
export function waitingToStart(job: Job | null): boolean {
  return job?.status === 'running' && job.tasks.every(task => task.state === 'pending')
}

/** 每个账号的行状态：检测中优先，其次排队；该账号的题都结束后显示最后一条说明。 */
export function rowStatuses(job: Job | null): Record<string, RowStatus> {
  const statuses: Record<string, RowStatus> = {}
  if (!job)
    return statuses
  const notes = new Map<string, string>()
  for (const task of job.tasks) {
    const current = statuses[task.account_id]
    if (task.state === 'running')
      statuses[task.account_id] = 'running'
    else if (task.state === 'pending' && current !== 'running')
      statuses[task.account_id] = 'queued'
    if (task.note)
      notes.set(task.account_id, task.state === 'skipped' ? `未执行：${task.note}` : task.note)
  }
  for (const [accountId, note] of notes) {
    if (!statuses[accountId])
      statuses[accountId] = { note }
  }
  return statuses
}

/** 把任务里已完成的结果并入账号的最近结果：按时间倒序、去重、最多 20 条。 */
export function mergeRecent(accounts: Account[], job: Job | null): Account[] {
  if (!job)
    return accounts
  return accounts.map((account) => {
    const known = new Set(account.recent.map(item => item.id))
    const fresh = job.tasks
      .filter(task => task.account_id === account.id && task.result && !known.has(task.result.id))
      .map(task => task.result!)
      .map(({ id, verdict, created_at_ms }) => ({ id, verdict, created_at_ms }))
    if (fresh.length === 0)
      return account
    const recent = [...fresh, ...account.recent]
      .sort((left, right) => right.created_at_ms - left.created_at_ms)
      .slice(0, KEEP)
    return { ...account, recent }
  })
}

/** 通过率 = 正常 /（正常 + 未通过）；"错误"不计入，分母为 0 时返回 null。 */
export function passRate(verdicts: readonly Verdict[]): number | null {
  const normal = verdicts.filter(verdict => verdict === 'normal').length
  const failed = verdicts.filter(verdict => verdict === 'failed').length
  return normal + failed === 0 ? null : normal / (normal + failed)
}
