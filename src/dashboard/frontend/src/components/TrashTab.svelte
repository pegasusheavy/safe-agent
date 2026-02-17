<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';

    interface TrashEntry {
        id: string;
        original_path: string;
        name: string;
        trashed_at: string;
        size_bytes: number;
        is_dir: boolean;
        source: string;
    }

    interface TrashStats {
        count: number;
        total_bytes: number;
    }

    interface TrashListResponse {
        items: TrashEntry[];
        stats: TrashStats;
    }

    interface ActionResponse {
        ok: boolean;
        message?: string;
        count?: number;
    }

    let items = $state<TrashEntry[]>([]);
    let stats = $state<TrashStats>({ count: 0, total_bytes: 0 });
    let loading = $state(false);
    let error = $state('');
    let successMsg = $state('');
    let confirmEmpty = $state(false);
    let searchQuery = $state('');

    async function load() {
        loading = true;
        error = '';
        try {
            const data = await api<TrashListResponse>('GET', '/api/trash');
            items = data.items;
            stats = data.stats;
        } catch (e: any) {
            error = e?.message || 'Failed to load trash';
            console.error('loadTrash:', e);
        } finally {
            loading = false;
        }
    }

    async function restore(id: string) {
        try {
            const res = await api<ActionResponse>('POST', `/api/trash/${id}/restore`);
            successMsg = res.message || 'Restored';
            await load();
            setTimeout(() => successMsg = '', 3000);
        } catch (e: any) {
            error = e?.message || 'Failed to restore';
        }
    }

    async function permanentDelete(id: string, name: string) {
        if (!confirm(`Permanently delete "${name}"? This cannot be undone.`)) return;
        try {
            const res = await api<ActionResponse>('DELETE', `/api/trash/${id}`);
            successMsg = res.message || 'Deleted';
            await load();
            setTimeout(() => successMsg = '', 3000);
        } catch (e: any) {
            error = e?.message || 'Failed to delete';
        }
    }

    async function emptyTrash() {
        try {
            const res = await api<ActionResponse>('POST', '/api/trash/empty');
            successMsg = res.message || 'Trash emptied';
            confirmEmpty = false;
            await load();
            setTimeout(() => successMsg = '', 3000);
        } catch (e: any) {
            error = e?.message || 'Failed to empty trash';
        }
    }

    function formatSize(bytes: number): string {
        if (bytes === 0) return '0 B';
        const units = ['B', 'KB', 'MB', 'GB'];
        const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
        const val = bytes / Math.pow(1024, i);
        return `${val.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
    }

    function formatDate(iso: string): string {
        try {
            const d = new Date(iso);
            const now = new Date();
            const diffMs = now.getTime() - d.getTime();
            const diffMins = Math.floor(diffMs / 60000);
            const diffHrs = Math.floor(diffMs / 3600000);
            const diffDays = Math.floor(diffMs / 86400000);

            if (diffMins < 1) return 'just now';
            if (diffMins < 60) return `${diffMins}m ago`;
            if (diffHrs < 24) return `${diffHrs}h ago`;
            if (diffDays < 7) return `${diffDays}d ago`;
            return d.toLocaleDateString();
        } catch {
            return iso;
        }
    }

    function sourceLabel(source: string): string {
        if (source.startsWith('tool:')) return source.replace('tool:', '');
        if (source.startsWith('shell:')) return source.replace('shell:', '') + ' (shell)';
        if (source.startsWith('rhai:')) return source.replace('rhai:', '') + ' (skill)';
        return source;
    }

    let filtered = $derived(
        searchQuery.trim()
            ? items.filter(i =>
                i.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
                i.original_path.toLowerCase().includes(searchQuery.toLowerCase()) ||
                i.source.toLowerCase().includes(searchQuery.toLowerCase())
            )
            : items
    );

    $effect(() => {
        if (dashboard.currentTab === 'trash') {
            dashboard.refreshCounter;
            load();
        }
    });
</script>

<section class="space-y-4">
    <!-- Header with stats -->
    <div class="bg-surface border border-border rounded-lg shadow-sm p-4">
        <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
                <h2 class="text-xs font-semibold uppercase tracking-wider text-text-muted">
                    <i class="fa-solid fa-trash-can mr-1.5"></i> Trash
                </h2>
                <span class="text-xs text-text-subtle bg-surface-muted px-2 py-0.5 rounded-full">
                    {stats.count} item{stats.count !== 1 ? 's' : ''} &middot; {formatSize(stats.total_bytes)}
                </span>
            </div>
            <div class="flex items-center gap-2">
                <button
                    class="text-xs px-3 py-1.5 rounded bg-surface-muted hover:bg-surface border border-border text-text-muted hover:text-text transition-colors"
                    onclick={load}
                    disabled={loading}
                >
                    <i class="fa-solid fa-arrows-rotate mr-1" class:fa-spin={loading}></i> Refresh
                </button>
                {#if stats.count > 0}
                    {#if !confirmEmpty}
                        <button
                            class="text-xs px-3 py-1.5 rounded bg-red-500/10 hover:bg-red-500/20 border border-red-500/30 text-red-400 hover:text-red-300 transition-colors"
                            onclick={() => confirmEmpty = true}
                        >
                            <i class="fa-solid fa-trash mr-1"></i> Empty Trash
                        </button>
                    {:else}
                        <div class="flex items-center gap-1.5">
                            <span class="text-xs text-red-400">Delete all?</span>
                            <button
                                class="text-xs px-2 py-1 rounded bg-red-600 hover:bg-red-700 text-white transition-colors"
                                onclick={emptyTrash}
                            >Yes</button>
                            <button
                                class="text-xs px-2 py-1 rounded bg-surface-muted hover:bg-surface border border-border text-text-muted transition-colors"
                                onclick={() => confirmEmpty = false}
                            >No</button>
                        </div>
                    {/if}
                {/if}
            </div>
        </div>
    </div>

    <!-- Notifications -->
    {#if error}
        <div class="bg-red-500/10 border border-red-500/30 text-red-400 text-sm rounded-lg px-4 py-2">
            <i class="fa-solid fa-circle-exclamation mr-1.5"></i> {error}
        </div>
    {/if}
    {#if successMsg}
        <div class="bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 text-sm rounded-lg px-4 py-2">
            <i class="fa-solid fa-check-circle mr-1.5"></i> {successMsg}
        </div>
    {/if}

    <!-- Search -->
    {#if items.length > 0}
        <div class="relative">
            <i class="fa-solid fa-search absolute left-3 top-1/2 -translate-y-1/2 text-text-subtle text-xs"></i>
            <input
                type="text"
                bind:value={searchQuery}
                placeholder="Search by name, path, or source..."
                class="w-full pl-9 pr-3 py-2 text-sm bg-surface border border-border rounded-lg text-text placeholder:text-text-subtle focus:outline-none focus:ring-1 focus:ring-primary-500 focus:border-primary-500"
            />
        </div>
    {/if}

    <!-- Items list -->
    <div class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
        {#if loading && items.length === 0}
            <div class="p-8 text-center text-text-subtle text-sm">
                <i class="fa-solid fa-spinner fa-spin mr-1.5"></i> Loading...
            </div>
        {:else if filtered.length === 0}
            <div class="p-8 text-center text-text-subtle text-sm">
                {#if searchQuery.trim()}
                    <i class="fa-solid fa-search mr-1.5"></i> No items matching "{searchQuery}"
                {:else}
                    <i class="fa-solid fa-trash-can mr-1.5 opacity-40"></i>
                    <p class="mt-1">Trash is empty</p>
                    <p class="text-xs mt-1 text-text-subtle opacity-70">
                        Deleted files and directories will appear here for recovery.
                    </p>
                {/if}
            </div>
        {:else}
            <div class="max-h-[600px] overflow-y-auto custom-scroll divide-y divide-border">
                {#each filtered as entry (entry.id)}
                    <div class="p-3 hover:bg-surface-muted/50 transition-colors group">
                        <div class="flex items-start justify-between gap-3">
                            <div class="flex-1 min-w-0">
                                <div class="flex items-center gap-2 mb-1">
                                    <i class="fa-solid {entry.is_dir ? 'fa-folder text-amber-400' : 'fa-file text-blue-400'} text-sm"></i>
                                    <span class="text-sm font-medium text-text truncate">{entry.name}</span>
                                    <span class="text-xs text-text-subtle bg-surface-muted px-1.5 py-0.5 rounded">
                                        {formatSize(entry.size_bytes)}
                                    </span>
                                </div>
                                <div class="text-xs text-text-subtle truncate pl-6" title={entry.original_path}>
                                    {entry.original_path}
                                </div>
                                <div class="flex items-center gap-3 mt-1 pl-6">
                                    <span class="text-xs text-text-subtle" title={entry.trashed_at}>
                                        <i class="fa-regular fa-clock mr-0.5"></i> {formatDate(entry.trashed_at)}
                                    </span>
                                    <span class="text-xs text-text-subtle">
                                        <i class="fa-solid fa-tag mr-0.5"></i> {sourceLabel(entry.source)}
                                    </span>
                                </div>
                            </div>
                            <div class="flex items-center gap-1.5 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                                <button
                                    class="text-xs px-2.5 py-1.5 rounded bg-emerald-500/10 hover:bg-emerald-500/20 border border-emerald-500/30 text-emerald-400 hover:text-emerald-300 transition-colors"
                                    title="Restore to original location"
                                    onclick={() => restore(entry.id)}
                                >
                                    <i class="fa-solid fa-rotate-left mr-1"></i> Restore
                                </button>
                                <button
                                    class="text-xs px-2.5 py-1.5 rounded bg-red-500/10 hover:bg-red-500/20 border border-red-500/30 text-red-400 hover:text-red-300 transition-colors"
                                    title="Permanently delete"
                                    onclick={() => permanentDelete(entry.id, entry.name)}
                                >
                                    <i class="fa-solid fa-xmark"></i>
                                </button>
                            </div>
                        </div>
                    </div>
                {/each}
            </div>
        {/if}
    </div>
</section>
