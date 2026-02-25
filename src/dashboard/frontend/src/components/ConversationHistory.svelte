<script lang="ts">
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import { formatDateTime } from '../lib/time';

    interface HistoryMessage {
        id: number;
        role: string;
        content: string;
        user_id: string | null;
        created_at: string;
    }

    interface HistoryResponse {
        messages: HistoryMessage[];
        total: number;
    }

    let messages = $state<HistoryMessage[]>([]);
    let total = $state(0);
    let query = $state('');
    let roleFilter = $state('all');
    let loading = $state(false);
    let offset = $state(0);
    const LIMIT = 50;

    async function load(append = false) {
        loading = true;
        try {
            const params = new URLSearchParams({ limit: String(LIMIT), offset: String(offset) });
            if (query) params.set('q', query);
            const data = await api<HistoryResponse>('GET', `/api/memory/conversation/history?${params}`);
            if (append) {
                messages = [...messages, ...data.messages];
            } else {
                messages = data.messages;
            }
            total = data.total;
        } catch (e) {
            console.error('loadHistory:', e);
        }
        loading = false;
    }

    function search() {
        offset = 0;
        load();
    }

    function loadMore() {
        offset += LIMIT;
        load(true);
    }

    function handleKey(e: KeyboardEvent) {
        if (e.key === 'Enter') search();
    }

    const filtered = $derived(
        roleFilter === 'all'
            ? messages
            : messages.filter(m => m.role === roleFilter)
    );

    function roleIcon(role: string): string {
        switch (role) {
            case 'user': return 'fa-user text-info-500';
            case 'assistant': return 'fa-robot text-primary-500';
            case 'system': return 'fa-gear text-text-subtle';
            default: return 'fa-circle text-text-subtle';
        }
    }

    load();
</script>

<section class="card">
    <div class="card__header">
        <h2 class="card__header-title">
            <i class="fa-solid fa-clock-rotate-left mr-1.5"></i> {t('history.title')}
        </h2>
        <span class="text-xs text-text-muted">
            {t('history.total', { total })}
        </span>
    </div>

    <div class="px-4 py-2 border-b border-border flex flex-col sm:flex-row gap-2">
        <input
            type="text"
            bind:value={query}
            onkeyup={handleKey}
            placeholder={t('history.search')}
            class="form__input flex-1"
        />
        <div class="flex gap-1">
            {#each [['all', t('history.all_roles')], ['user', t('history.user_only')], ['assistant', t('history.assistant_only')], ['system', t('history.system_only')]] as [value, label]}
                <button
                    onclick={() => roleFilter = value}
                    class="px-2 py-1 text-xs rounded border transition-colors"
                    class:bg-primary-600={roleFilter === value}
                    class:text-white={roleFilter === value}
                    class:border-primary-600={roleFilter === value}
                    class:border-border={roleFilter !== value}
                    class:text-text-muted={roleFilter !== value}
                    class:hover:text-text={roleFilter !== value}
                >
                    {label}
                </button>
            {/each}
        </div>
    </div>

    <div class="max-h-[600px] overflow-y-auto custom-scroll divide-y divide-border-muted">
        {#if filtered.length === 0 && !loading}
            <p class="text-text-subtle text-sm italic text-center py-6">{t('history.no_results')}</p>
        {/if}
        {#each filtered as msg (msg.id)}
            <div class="px-4 py-3 hover:bg-surface-muted/30 transition-colors">
                <div class="flex items-center gap-2 mb-1">
                    <i class="fa-solid {roleIcon(msg.role)} text-xs w-4 text-center"></i>
                    <span class="text-xs font-medium text-text-muted uppercase tracking-wide">{msg.role}</span>
                    {#if msg.user_id}
                        <span class="text-[10px] text-text-subtle">({msg.user_id})</span>
                    {/if}
                    <span class="text-[10px] text-text-subtle ml-auto">{formatDateTime(msg.created_at)}</span>
                </div>
                <div class="text-sm text-text whitespace-pre-wrap break-words leading-relaxed pl-6">
                    {msg.content.length > 500 ? msg.content.slice(0, 500) + '…' : msg.content}
                </div>
            </div>
        {/each}
    </div>

    {#if messages.length < total}
        <div class="px-4 py-3 border-t border-border text-center">
            <button
                onclick={loadMore}
                disabled={loading}
                class="btn btn--secondary btn--md disabled:opacity-50"
            >
                {#if loading}
                    <i class="fa-solid fa-spinner fa-spin mr-1"></i>
                {/if}
                {t('history.load_more')}
            </button>
        </div>
    {/if}
</section>
