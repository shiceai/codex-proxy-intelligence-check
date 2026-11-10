// @env browser
export interface ManagementReply {
  status: number
  contentType: string
  body: ArrayBuffer
}

export interface ManagementRequestInput {
  method: 'GET' | 'POST'
  path: string
  query?: string
  contentType?: string
  body?: string
}

/** 宿主注入的隔离页桥，只暴露本插件用到的部分。 */
export interface PluginHost {
  readonly version: 2
  readonly theme: 'light' | 'dark'
  request: (input: ManagementRequestInput) => Promise<ManagementReply>
}

export function getHost(): PluginHost {
  const host = window.codexProxyPlugin
  if (!host || host.version !== 2)
    throw new Error('请从网关的插件管理页面打开')
  return host
}
