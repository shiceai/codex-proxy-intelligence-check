import type { Account, Job, JobTask } from '../src/api/types.ts'
import assert from 'node:assert/strict'
import { it } from 'node:test'
import { isActive, mergeRecent, passRate, progress, rowStatuses, waitingToStart } from '../src/utils/jobs.ts'

function task(accountId: string, state: JobTask['state'], extra: Partial<JobTask> = {}): JobTask {
  return { account_id: accountId, question_id: 'candy', state, result: null, note: null, ...extra }
}

function job(tasks: JobTask[], status: Job['status'] = 'running'): Job {
  return {
    id: 'j1',
    created_at_ms: 1,
    finished_at_ms: null,
    key_id: 'key_1',
    key_name: '主 Key',
    model: 'gpt-6-astra',
    effort: 'high',
    status,
    tasks,
  }
}

it('进行中或停止中的任务是活动任务', () => {
  assert.equal(isActive(job([], 'running')), true)
  assert.equal(isActive(job([], 'stopping')), true)
  assert.equal(isActive(job([], 'finished')), false)
  assert.equal(isActive(null), false)
})

it('进度统计已结束的题', () => {
  const value = job([task('a', 'done'), task('a', 'running'), task('b', 'skipped'), task('c', 'pending')])
  assert.deepEqual(progress(value), { done: 2, total: 4 })
  assert.deepEqual(progress(null), { done: 0, total: 0 })
})

it('行状态：检测中优先，其次排队；账号的题都结束后显示说明', () => {
  const value = job([
    task('a', 'done'),
    task('a', 'running'),
    task('b', 'pending'),
    task('c', 'skipped', { note: '已停止' }),
    task('d', 'interrupted', { note: '检测被中断：插件停用、升级或宿主重启' }),
    task('e', 'done'),
  ])
  assert.deepEqual(rowStatuses(value), {
    a: 'running',
    b: 'queued',
    c: { note: '未执行：已停止' },
    d: { note: '检测被中断：插件停用、升级或宿主重启' },
  })
})

it('结果未保存时在行上提示', () => {
  const value = job([task('a', 'done', { note: '结果写入失败，未保存', result: { id: 'r1', verdict: 'normal', created_at_ms: 5, saved: false } })])
  assert.deepEqual(rowStatuses(value), { a: { note: '结果写入失败，未保存' } })
})

it('任务完成的结果并入色块条，按时间倒序、去重、最多 20 条', () => {
  const accounts: Account[] = [
    { id: 'a', label: 'a', plan_type: null, recent: [{ id: 'r0', verdict: 'failed', created_at_ms: 1 }] },
    { id: 'b', label: 'b', plan_type: null, recent: [] },
  ]
  const value = job([
    task('a', 'done', { result: { id: 'r1', verdict: 'normal', created_at_ms: 5, saved: true } }),
    task('a', 'done', { result: { id: 'r0', verdict: 'failed', created_at_ms: 1, saved: true } }),
  ])
  const merged = mergeRecent(accounts, value)
  assert.deepEqual(merged[0]?.recent.map(item => item.id), ['r1', 'r0'])
  assert.deepEqual(merged[1]?.recent, [])
  const many: Account[] = [{ id: 'a', label: 'a', plan_type: null, recent: Array.from({ length: 20 }, (_, index) => ({ id: `o${index}`, verdict: 'normal' as const, created_at_ms: 0 })) }]
  assert.equal(mergeRecent(many, value)[0]?.recent.length, 20)
  assert.equal(mergeRecent(many, value)[0]?.recent[0]?.id, 'r1')
})

it('所有题都还在排队时提示等待后台开始', () => {
  assert.equal(waitingToStart(job([task('a', 'pending'), task('b', 'pending')])), true)
  assert.equal(waitingToStart(job([task('a', 'running'), task('b', 'pending')])), false)
  assert.equal(waitingToStart(job([task('a', 'pending')], 'stopping')), false)
})

it('passRate', () => {
  assert.equal(passRate(['normal', 'failed', 'error', 'normal']), 2 / 3)
  assert.equal(passRate(['error']), null)
  assert.equal(passRate([]), null)
})
