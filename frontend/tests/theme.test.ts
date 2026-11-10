import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { it } from 'node:test'

// 宿主 v3.37.0 起按 UI v0.8.0 的 `z` 命名同步主题，旧的 `cp` 类名与变量不再生效。
const OLD_NAMESPACE = /(?:^|[^a-z0-9])cp-[a-z]|-cp(?:-|(?![a-z0-9]))/

function sources(directory: string): string[] {
  return readdirSync(directory, { recursive: true, encoding: 'utf8' })
    .filter(path => /\.(?:vue|ts|css)$/.test(path))
    .map(path => join(directory, path))
}

it('页面不再使用旧的 cp 主题命名', () => {
  const offending = [...sources('src'), 'index.html'].flatMap(file =>
    readFileSync(file, 'utf8')
      .split('\n')
      .flatMap((line, index) => OLD_NAMESPACE.test(line) ? [`${file}:${index + 1}: ${line.trim()}`] : []),
  )
  assert.deepEqual(offending, [])
})
