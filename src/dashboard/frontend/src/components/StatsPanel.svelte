<script lang="ts">
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import type { AgentStats } from '../lib/types';

    let stats = $state<AgentStats | null>(null);

    async function load() {
        try {
            stats = await api<AgentStats>('GET', '/api/stats');
        } catch (e) {
            console.error('loadStats:', e);
        }
    }

    $effect(() => {
        dashboard.refreshCounter;
        load();
    });
</script>

<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted border-b border-border">
        <i class="fa-solid fa-chart-bar mr-1.5"></i> {t('stats.title')}
    </h2>
    <div class="p-3 max-h-96 overflow-y-auto custom-scroll">
        {#if stats}
            <div class="grid grid-cols-2 gap-3">
                <div class="text-center">
                    <div class="text-3xl font-bold text-primary-500">{stats.total_ticks}</div>
                    <div class="text-[11px] text-text-muted uppercase tracking-wider mt-0.5">{t('stats.ticks')}</div>
                </div>
                <div class="text-center">
                    <div class="text-3xl font-bold text-primary-500">{stats.total_actions}</div>
                    <div class="text-[11px] text-text-muted uppercase tracking-wider mt-0.5">{t('stats.messages')}</div>
                </div>
                <div class="text-center">
                    <div class="text-3xl font-bold text-success-500">{stats.total_approved}</div>
                    <div class="text-[11px] text-text-muted uppercase tracking-wider mt-0.5">{t('stats.approvals')}</div>
                </div>
                <div class="text-center">
                    <div class="text-3xl font-bold text-error-500">{stats.total_rejected}</div>
                    <div class="text-[11px] text-text-muted uppercase tracking-wider mt-0.5">{t('stats.rejections')}</div>
                </div>
            </div>
            <div class="mt-3 text-xs text-text-muted">
                {#if stats.last_tick_at}
                    <i class="fa-regular fa-clock mr-1"></i>{t('stats.last_tick')}: {stats.last_tick_at}<br>
                {:else}
                    No ticks yet<br>
                {/if}
                <i class="fa-solid fa-power-off mr-1"></i>Started: {stats.started_at}
            </div>
        {:else}
            <p class="text-text-subtle text-sm italic text-center py-4">Loading...</p>
        {/if}
    </div>
</section>
