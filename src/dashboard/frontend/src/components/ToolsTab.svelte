<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import type { ToolInfo } from '../lib/types';

    let tools = $state<ToolInfo[]>([]);

    async function load() {
        try {
            tools = await api<ToolInfo[]>('GET', '/api/tools');
        } catch (e) {
            console.error('loadTools:', e);
        }
    }

    $effect(() => {
        if (dashboard.currentTab === 'tools') {
            dashboard.refreshCounter;
            load();
        }
    });
</script>

<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted border-b border-border">
        <i class="fa-solid fa-screwdriver-wrench mr-1.5"></i> Registered Tools
    </h2>
    <div class="p-3 max-h-[600px] overflow-y-auto custom-scroll">
        {#if tools.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">No tools registered</p>
        {:else}
            {#each tools as t (t.name)}
                <div class="p-3 border border-border rounded-md mb-2 bg-surface-muted">
                    <div class="text-xs uppercase tracking-wider text-primary-500 font-semibold mb-1">
                        <i class="fa-solid fa-wrench mr-1"></i>{t.name}
                    </div>
                    <div class="text-sm text-text-muted">{t.description}</div>
                </div>
            {/each}
        {/if}
    </div>
</section>
