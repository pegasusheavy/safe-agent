<script lang="ts">
    import { t } from '../lib/i18n';
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

<section class="card">
    <div class="card__header">
        <h2 class="card__header-title">
            <i class="fa-solid fa-clock-rotate-left mr-1.5"></i> {t('pending.title')}
        </h2>
        <div class="flex gap-1.5 pr-3">
            <button
                onclick={approveAll}
                class="btn btn--success btn--sm"
            >{t('pending.approve_all')}</button>
            <button
                onclick={rejectAll}
                class="btn btn--danger btn--sm"
            >{t('pending.reject_all')}</button>
        </div>
    </div>
    <div class="p-3 max-h-96 overflow-y-auto custom-scroll">
        {#if actions.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">{t('pending.no_pending')}</p>
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
                            class="btn btn--success btn--sm"
                        >
                            <i class="fa-solid fa-check mr-1"></i>{t('pending.approve')}
                        </button>
                        <button
                            onclick={() => reject(a.id)}
                            class="btn btn--danger btn--sm"
                        >
                            <i class="fa-solid fa-xmark mr-1"></i>{t('pending.reject')}
                        </button>
                    </div>
                </div>
            {/each}
        {/if}
    </div>
</section>
