<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard, refreshAll } from '../lib/state.svelte';
    import type { PendingAction, ActionResponse } from '../lib/types';

    let actions = $state<PendingAction[]>([]);

    async function load() {
        try {
            actions = await api<PendingAction[]>('GET', '/api/pending');
        } catch (e) {
            console.error('loadPending:', e);
        }
    }

    async function approve(id: string) {
        await api<ActionResponse>('POST', `/api/pending/${id}/approve`);
        refreshAll();
    }

    async function reject(id: string) {
        await api<ActionResponse>('POST', `/api/pending/${id}/reject`);
        refreshAll();
    }

    async function approveAll() {
        await api<ActionResponse>('POST', '/api/pending/approve-all');
        refreshAll();
    }

    async function rejectAll() {
        await api<ActionResponse>('POST', '/api/pending/reject-all');
        refreshAll();
    }

    function formatParams(a: PendingAction): string {
        const params = a.action?.params;
        return params ? JSON.stringify(params, null, 2) : '{}';
    }

    $effect(() => {
        dashboard.refreshCounter;
        load();
    });
</script>

<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-clock-rotate-left mr-1.5"></i> Pending Actions
        </h2>
        <div class="flex gap-1.5 pr-3">
            <button
                onclick={approveAll}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface text-success-500 hover:bg-success-500/10 hover:border-success-500 transition-colors"
            >Approve All</button>
            <button
                onclick={rejectAll}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface text-error-500 hover:bg-error-500/10 hover:border-error-500 transition-colors"
            >Reject All</button>
        </div>
    </div>
    <div class="p-3 max-h-96 overflow-y-auto custom-scroll">
        {#if actions.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">No pending actions</p>
        {:else}
            {#each actions as a (a.id)}
                <div class="p-3 border border-border rounded-md mb-2 bg-surface-muted">
                    <div class="text-xs uppercase tracking-wider text-primary-500 font-semibold mb-1">
                        <i class="fa-solid fa-gear mr-1"></i>{a.action?.tool ?? 'unknown'}
                    </div>
                    <pre class="text-xs whitespace-pre-wrap my-1 text-text font-mono">{formatParams(a)}</pre>
                    <div class="text-xs text-text-muted mb-2">{a.reasoning ?? a.action?.reasoning ?? ''}</div>
                    <div class="text-[11px] text-text-subtle">{a.proposed_at}</div>
                    <div class="flex gap-1.5 mt-2">
                        <button
                            onclick={() => approve(a.id)}
                            class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface text-success-500 hover:bg-success-500/10 hover:border-success-500 transition-colors"
                        >
                            <i class="fa-solid fa-check mr-1"></i>Approve
                        </button>
                        <button
                            onclick={() => reject(a.id)}
                            class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface text-error-500 hover:bg-error-500/10 hover:border-error-500 transition-colors"
                        >
                            <i class="fa-solid fa-xmark mr-1"></i>Reject
                        </button>
                    </div>
                </div>
            {/each}
        {/if}
    </div>
</section>
