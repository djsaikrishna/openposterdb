import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import DashboardView from '@/views/DashboardView.vue'

const mockAdminApi = vi.hoisted(() => ({
  getStats: vi.fn(),
  purgeAll: vi.fn(),
}))

vi.mock('@/lib/api', () => ({
  adminApi: mockAdminApi,
}))

const sampleStats = {
  total_images: 42,
  total_api_keys: 3,
  mem_cache_entries: 100,
  id_cache_entries: 50,
  ratings_cache_entries: 25,
  image_mem_cache_mb: 128,
}

function mountView() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return mount(DashboardView, {
    global: {
      plugins: [createPinia(), [VueQueryPlugin, { queryClient }]],
      stubs: {
        Button: {
          template: '<button @click="$emit(\'click\')" :disabled="disabled"><slot /></button>',
          props: ['disabled', 'variant', 'size'],
        },
        Card: { template: '<div><slot /></div>' },
        CardHeader: { template: '<div><slot /></div>' },
        CardTitle: { template: '<div><slot /></div>' },
        CardContent: { template: '<div><slot /></div>' },
        Skeleton: { template: '<div data-testid="skeleton" />' },
        RefreshCw: { template: '<span />' },
        Trash2: { template: '<span />' },
        Loader2: { template: '<span />' },
        Dialog: { template: '<div v-if="open"><slot /></div>', props: ['open'] },
        DialogContent: { template: '<div><slot /></div>' },
        DialogHeader: { template: '<div><slot /></div>' },
        DialogTitle: { template: '<div><slot /></div>' },
      },
    },
  })
}

describe('DashboardView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('shows skeletons while loading', () => {
    mockAdminApi.getStats.mockReturnValue(new Promise(() => {})) // never resolves
    const wrapper = mountView()
    expect(wrapper.findAll('[data-testid="skeleton"]').length).toBeGreaterThan(0)
  })

  it('renders stats from API response', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('42')
    expect(wrapper.text()).toContain('3')
    expect(wrapper.text()).toContain('100')
    expect(wrapper.text()).toContain('128')
  })

  it('has a refresh button', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })

    const wrapper = mountView()
    await flushPromises()

    const refreshButton = wrapper.findAll('button').find((b) => b.text().includes('Refresh'))
    expect(refreshButton).toBeDefined()
  })

  it('refresh button triggers refetch', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })

    const wrapper = mountView()
    await flushPromises()

    mockAdminApi.getStats.mockClear()
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ ...sampleStats, total_images: 99 }),
    })

    const refreshButton = wrapper.findAll('button').find((b) => b.text().includes('Refresh'))
    await refreshButton!.trigger('click')
    await flushPromises()

    expect(mockAdminApi.getStats).toHaveBeenCalled()
    expect(wrapper.text()).toContain('99')
  })

  it('clears the cache through the confirm dialog', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })
    mockAdminApi.purgeAll.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ ok: true, external_cache_only: false, dirs_removed: 4, meta_deleted: 42, ratings_deleted: 10 }),
    })

    const wrapper = mountView()
    await flushPromises()

    // Open the confirm dialog.
    const trigger = wrapper.findAll('button').find((b) => b.text().includes('Clear cache'))
    expect(trigger).toBeDefined()
    await trigger!.trigger('click')
    await flushPromises()

    // The dialog adds a second "Clear cache" button (the destructive confirm).
    const confirmButtons = wrapper.findAll('button').filter((b) => b.text().trim() === 'Clear cache')
    expect(confirmButtons.length).toBe(2)
    await confirmButtons[confirmButtons.length - 1].trigger('click')
    await flushPromises()

    expect(mockAdminApi.purgeAll).toHaveBeenCalled()
    expect(wrapper.text()).toContain('Cache cleared')
  })
})
