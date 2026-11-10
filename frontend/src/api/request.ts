import { getHost } from './host'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

// path 是插件注册的相对路由；会话与实例定位由宿主负责，不直接访问管理端 HTTP 接口。
export default async function request<T>(method: 'GET' | 'POST', path: string, data?: unknown): Promise<T> {
  const reply = await getHost().request({
    method,
    path,
    ...(data === undefined ? {} : { contentType: 'application/json', body: JSON.stringify(data) }),
  })
  if (reply.contentType.split(';', 1)[0]?.trim().toLowerCase() !== 'application/json')
    throw new Error('插件返回了不支持的内容类型')

  let value: unknown
  try {
    value = JSON.parse(new TextDecoder().decode(reply.body))
  }
  catch (error) {
    throw new Error('插件返回了无效 JSON', { cause: error })
  }
  if (reply.status < 200 || reply.status >= 300) {
    const message = isRecord(value) && isRecord(value.error) && typeof value.error.message === 'string'
      ? value.error.message
      : `插件请求失败（HTTP ${reply.status}）`
    throw new Error(message)
  }
  return value as T
}
