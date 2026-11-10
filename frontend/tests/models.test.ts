import assert from 'node:assert/strict'
import { it } from 'node:test'
import { DEFAULT_MODEL, pickModel } from '../src/utils/models.ts'

it('默认选择 gpt-6-astra', () => {
  assert.equal(DEFAULT_MODEL, 'gpt-6-astra')
  assert.equal(pickModel(['gpt-5.5', 'gpt-6-astra', 'gpt-5.4'], null), 'gpt-6-astra')
})

it('切换 Key 后保留仍然可用的已选模型', () => {
  assert.equal(pickModel(['gpt-5.5', 'gpt-6-astra'], 'gpt-5.5'), 'gpt-5.5')
})

it('已选模型不可用时回到 gpt-6-astra', () => {
  assert.equal(pickModel(['gpt-5.4', 'gpt-6-astra'], 'gpt-5.5'), 'gpt-6-astra')
})

it('Key 看不到 gpt-6-astra 时选第一个模型', () => {
  assert.equal(pickModel(['gpt-5.5', 'gpt-5.4'], null), 'gpt-5.5')
  assert.equal(pickModel([], null), null)
})
