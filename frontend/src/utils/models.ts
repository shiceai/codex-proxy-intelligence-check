// 默认检测模型：糖果题以 gpt-6-astra 的表现作为判断基准。
export const DEFAULT_MODEL = 'gpt-6-astra'

/** 优先保留仍可用的已选模型，其次 gpt-6-astra，最后取列表第一个。 */
export function pickModel(models: readonly string[], current: string | null): string | null {
  if (current && models.includes(current))
    return current
  if (models.includes(DEFAULT_MODEL))
    return DEFAULT_MODEL
  return models[0] ?? null
}
