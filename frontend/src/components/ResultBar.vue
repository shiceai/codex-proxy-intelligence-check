<script setup lang="ts">
import type { Recent, Verdict } from '@/api'
import { computed } from 'vue'

const props = defineProps<{
  recent: Recent[]
}>()

const emit = defineEmits<{
  open: [resultId: string]
}>()

const SLOTS = 20
const labels: Record<Verdict, string> = { normal: '正常', failed: '未通过', error: '错误' }
const colors: Record<Verdict, string> = {
  normal: 'bg-z-success',
  failed: 'bg-z-error',
  error: 'bg-z-text-tertiary',
}

// 最新的结果在最右侧；不足 20 次时左侧用浅色占位。
const cells = computed<(Recent | null)[]>(() => {
  const items = props.recent.slice(0, SLOTS).reverse()
  const padding = Array.from<Recent | null>({ length: SLOTS - items.length }).fill(null)
  return [...padding, ...items]
})

function title(item: Recent) {
  return `${labels[item.verdict]} · ${new Date(item.created_at_ms).toLocaleString('zh-CN')}`
}
</script>

<template>
  <div class="flex items-center gap-0.5" role="list" aria-label="最近 20 次检测结果">
    <template v-for="(item, index) in cells" :key="item?.id ?? `empty-${index}`">
      <button
        v-if="item"
        type="button"
        role="listitem"
        class="h-4 w-1.5 cursor-pointer rounded-sm transition-opacity hover:opacity-70"
        :class="colors[item.verdict]"
        :title="title(item)"
        :aria-label="title(item)"
        @click="emit('open', item.id)"
      />
      <span v-else role="listitem" class="h-4 w-1.5 rounded-sm bg-z-fill-secondary" aria-hidden="true" />
    </template>
  </div>
</template>
