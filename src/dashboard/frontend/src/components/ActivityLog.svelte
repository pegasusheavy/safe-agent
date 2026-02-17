<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import { formatRelative } from '../lib/time';
    import type { ActivityEntry } from '../lib/types';

    let entries = $state<ActivityEntry[]>([]);

    async function load() {
        try {
            entries = await api<ActivityEntry[]>('GET', '/api/activity?limit=30');
        } catch (e) {
            console.error('loadActivity:', e);
        }
    }

    function dotColor(status: string): string {
        if (status === 'ok') return 'bg-success-500';
        if (status === 'error') return 'bg-error-500';
        return 'bg-warning-500';
    }

    $effect(() => {
        dashboard.refreshCounter;
        load();
    });
</script>

<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted border-b border-border">
        <i class="fa-solid fa-list-check mr-1.5"></i> Activity Log
    </h2>
    <div class="p-3 max-h-96 overflow-y-auto custom-scroll">
        {#if entries.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">No activity yet</p>
        {:else}
            {#each entries as e}
                <div class="flex items-start gap-2.5 py-2 border-b border-border-muted text-sm">
                    <div class="w-1.5 h-1.5 rounded-full mt-1.5 shrink-0 {dotColor(e.status)}"></div>
                    <div class="flex-1">
                        <strong>{e.action_type}</strong>: {e.summary}
                        {#if e.detail}
                            <br><span class="text-text-muted">{e.detail.slice(0, 200)}</span>
                        {/if}
                    </div>
                    <div class="text-[11px] text-text-subtle whitespace-nowrap">{formatRelative(e.created_at)}</div>
                </div>
            {/each}
        {/if}
    </div>
</section>
