<script setup lang="ts">
import { ref } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { Trash2, Loader2 } from 'lucide-vue-next'
import { adminApi } from '@/lib/api'
import RefreshButton from '@/components/RefreshButton.vue'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

interface Stats {
  total_images: number
  total_api_keys: number
  mem_cache_entries: number
  id_cache_entries: number
  ratings_cache_entries: number
  image_mem_cache_mb: number
}

const { data: stats, isPending, isFetching, refetch } = useQuery<Stats>({
  queryKey: ['admin', 'stats'],
  queryFn: async () => {
    const res = await adminApi.getStats()
    if (!res.ok) throw new Error('Failed to fetch stats')
    return res.json()
  },
})

const cards = [
  { key: 'total_images', label: 'Total Images' },
  { key: 'total_api_keys', label: 'API Keys' },
  { key: 'mem_cache_entries', label: 'Memory Cache Entries' },
  { key: 'id_cache_entries', label: 'ID Cache Entries' },
  { key: 'ratings_cache_entries', label: 'Ratings Cache Entries' },
  { key: 'image_mem_cache_mb', label: 'Image Cache (MB)' },
] as const

const clearOpen = ref(false)
const clearLoading = ref(false)
const clearError = ref('')
const clearMessage = ref('')

async function confirmClear() {
  clearLoading.value = true
  clearError.value = ''

  try {
    const res = await adminApi.purgeAll()
    if (!res.ok) {
      const text = await res.text()
      try { clearError.value = JSON.parse(text).error || text } catch { clearError.value = text || `Error ${res.status}` }
      return
    }
    const body = await res.json()
    clearMessage.value = body.external_cache_only
      ? 'Cache metadata cleared. Images are served from an external CDN and could not be purged from here.'
      : `Cache cleared — removed ${body.meta_deleted} cached image${body.meta_deleted === 1 ? '' : 's'}.`
    clearOpen.value = false
    refetch()
  } catch (e) {
    clearError.value = e instanceof Error ? e.message : 'Clear failed'
  } finally {
    clearLoading.value = false
  }
}

function openClear() {
  clearError.value = ''
  clearMessage.value = ''
  clearOpen.value = true
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-end gap-2">
      <Button variant="outline" size="sm" class="text-destructive hover:text-destructive" @click="openClear">
        <Trash2 class="size-4 mr-1" />
        Clear cache
      </Button>
      <RefreshButton :fetching="isFetching" @refresh="refetch()" />
    </div>
    <p v-if="clearMessage" class="text-sm text-muted-foreground text-right">{{ clearMessage }}</p>
    <div class="grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
    <Card v-for="card in cards" :key="card.key">
      <CardHeader class="pb-2">
        <CardTitle class="text-sm font-medium text-muted-foreground">{{ card.label }}</CardTitle>
      </CardHeader>
      <CardContent>
        <Skeleton v-if="isPending" class="h-8 w-20" />
        <p v-else class="text-2xl font-bold">{{ stats?.[card.key] ?? '—' }}</p>
      </CardContent>
    </Card>
    </div>

    <Dialog :open="clearOpen" @update:open="(v: boolean) => { if (!v) clearOpen = false }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>Clear cache</DialogTitle>
        </DialogHeader>
        <div class="space-y-4">
          <p class="text-sm text-muted-foreground">
            This removes every cached poster, logo, backdrop, and episode — rendered
            images, raw downloads, and in-memory caches. Images are regenerated on the
            next request, so the first load of each title will be slower.
          </p>
          <p v-if="clearError" class="text-sm text-destructive">{{ clearError }}</p>
          <div class="flex justify-end gap-2">
            <Button variant="outline" :disabled="clearLoading" @click="clearOpen = false">Cancel</Button>
            <Button variant="destructive" :disabled="clearLoading" @click="confirmClear">
              <Loader2 v-if="clearLoading" class="size-4 animate-spin mr-1" />
              Clear cache
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  </div>
</template>
