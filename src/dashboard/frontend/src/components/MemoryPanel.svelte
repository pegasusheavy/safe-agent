<script lang="ts">
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import { formatRelative } from '../lib/time';
    import type { CoreMemoryData, ConversationMessage, ArchivalEntry } from '../lib/types';

    let content = $state('');
    let messages = $state<ConversationMessage[]>([]);
    let archivalEntries = $state<ArchivalEntry[]>([]);
    let searchQuery = $state('');
    let error = $state(false);

    const memTabs = [
        { id: 'core' as const, label: 'Core', icon: 'fa-microchip' },
        { id: 'conversation' as const, label: 'Conversation', icon: 'fa-comments' },
        { id: 'archival' as const, label: 'Archival', icon: 'fa-box-archive' },
    ];

    function switchMemTab(id: typeof dashboard.currentMemoryTab) {
        dashboard.currentMemoryTab = id;
    }

    async function load() {
        error = false;
        try {
            if (dashboard.currentMemoryTab === 'core') {
                const data = await api<CoreMemoryData>('GET', '/api/memory/core');
                content = data.personality || '(empty)';
                messages = [];
                archivalEntries = [];
            } else if (dashboard.currentMemoryTab === 'conversation') {
                const data = await api<ConversationMessage[]>('GET', '/api/memory/conversation');
                messages = data;
                content = '';
                archivalEntries = [];
            } else if (dashboard.currentMemoryTab === 'archival') {
                const url = searchQuery
                    ? `/api/memory/archival?q=${encodeURIComponent(searchQuery)}`
                    : '/api/memory/archival';
                const data = await api<ArchivalEntry[]>('GET', url);
                archivalEntries = data;
                content = '';
                messages = [];
            }
        } catch (e) {
            error = true;
            console.error('loadMemory:', e);
        }
    }

    function handleSearchKey(e: KeyboardEvent) {
        if (e.key === 'Enter') load();
    }

    $effect(() => {
        dashboard.refreshCounter;
        dashboard.currentMemoryTab;
        load();
    });
</script>

<section class="card">
    <h2 class="card__header-title px-4 py-3 border-b border-border">
        <i class="fa-solid fa-brain mr-1.5"></i> Memory
    </h2>
    <div class="flex px-4 border-b border-border">
        {#each memTabs as tab}
            <button
                class="nav-tab nav-tab--sub"
                class:nav-tab--active={dashboard.currentMemoryTab === tab.id}
                onclick={() => switchMemTab(tab.id)}
            >
                <i class="fa-solid {tab.icon} mr-1"></i> {t('memory.' + tab.id)}
            </button>
        {/each}
    </div>

    {#if dashboard.currentMemoryTab === 'archival'}
        <div class="px-4 py-2 border-b border-border">
            <input
                type="text"
                bind:value={searchQuery}
                onkeyup={handleSearchKey}
                placeholder={t('memory.search_placeholder')}
                class="w-full px-2.5 py-1.5 border border-border rounded-md bg-background text-text text-sm outline-none focus:border-primary-500 focus:ring-1 focus:ring-primary-900 font-sans"
            />
        </div>
    {/if}

    <div class="p-3 max-h-96 overflow-y-auto custom-scroll">
        {#if error}
            <p class="text-text-subtle text-sm italic text-center py-4">Error loading memory</p>
        {:else if dashboard.currentMemoryTab === 'core'}
            <div class="whitespace-pre-wrap font-mono text-xs leading-relaxed">{content}</div>
        {:else if dashboard.currentMemoryTab === 'conversation'}
            {#if messages.length === 0}
                <p class="text-text-subtle text-sm italic text-center py-4">{t('memory.empty')}</p>
            {:else}
                {#each messages as m}
                    <div class="py-1.5 border-b border-border-muted text-sm">
                        <strong>{m.role}</strong>
                        <span class="text-text-muted">{formatRelative(m.created_at)}</span><br>
                        {m.content}
                    </div>
                {/each}
            {/if}
        {:else if dashboard.currentMemoryTab === 'archival'}
            {#if archivalEntries.length === 0}
                <p class="text-text-subtle text-sm italic text-center py-4">{t('memory.empty')}</p>
            {:else}
                {#each archivalEntries as e}
                    <div class="py-1.5 border-b border-border-muted text-sm">
                        <span class="text-xs text-primary-500 font-semibold uppercase tracking-wider">{e.category}</span>
                        <span class="text-text-muted text-[11px]"> {formatRelative(e.created_at)}</span><br>
                        {e.content}
                    </div>
                {/each}
            {/if}
        {/if}
    </div>
</section>
