<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import { formatDateTime } from '../lib/time';
    import type { KnowledgeStats, KnowledgeNode, KnowledgeNeighbor } from '../lib/types';

    let stats = $state<KnowledgeStats>({ nodes: 0, edges: 0 });
    let nodes = $state<KnowledgeNode[]>([]);
    let neighbors = $state<KnowledgeNeighbor[]>([]);
    let viewingNeighbors = $state(false);
    let searchQuery = $state('');

    async function loadAll() {
        try {
            const [s, n] = await Promise.all([
                api<KnowledgeStats>('GET', '/api/knowledge/stats'),
                api<KnowledgeNode[]>('GET', '/api/knowledge/nodes?limit=100'),
            ]);
            stats = s;
            nodes = n;
            viewingNeighbors = false;
        } catch (e) {
            console.error('loadKnowledge:', e);
        }
    }

    async function search() {
        if (!searchQuery) {
            loadAll();
            return;
        }
        try {
            nodes = await api<KnowledgeNode[]>(
                'GET',
                `/api/knowledge/search?q=${encodeURIComponent(searchQuery)}`,
            );
            viewingNeighbors = false;
        } catch (e) {
            console.error('searchKnowledge:', e);
        }
    }

    async function viewNode(id: number) {
        try {
            const data = await api<KnowledgeNeighbor[]>(
                'GET',
                `/api/knowledge/nodes/${id}/neighbors`,
            );
            if (!data.length) {
                alert('No neighbors for this node.');
                return;
            }
            neighbors = data;
            viewingNeighbors = true;
        } catch (e) {
            console.error('loadNodeNeighbors:', e);
        }
    }

    function backToNodes() {
        viewingNeighbors = false;
    }

    function handleSearchKey(e: KeyboardEvent) {
        if (e.key === 'Enter') search();
    }

    $effect(() => {
        if (dashboard.currentTab === 'knowledge') {
            dashboard.refreshCounter;
            loadAll();
        }
    });
</script>

<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-diagram-project mr-1.5"></i> Knowledge Graph
        </h2>
        <div class="flex items-center gap-2 pr-3">
            <span class="text-xs text-text-muted">
                <i class="fa-solid fa-circle-nodes mr-1"></i>{stats.nodes} nodes, {stats.edges} edges
            </span>
        </div>
    </div>
    <div class="px-4 py-2 border-b border-border">
        <input
            type="text"
            bind:value={searchQuery}
            onkeyup={handleSearchKey}
            placeholder="Search knowledge graph..."
            class="w-full px-2.5 py-1.5 border border-border rounded-md bg-background text-text text-sm outline-none focus:border-primary-500 focus:ring-1 focus:ring-primary-900 font-sans"
        />
    </div>
    <div class="p-3 max-h-[600px] overflow-y-auto custom-scroll">
        {#if viewingNeighbors}
            <button
                onclick={backToNodes}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors mb-3"
            >
                <i class="fa-solid fa-arrow-left mr-1"></i> Back
            </button>
            {#each neighbors as n}
                <div class="p-3 border border-border rounded-md mb-2 bg-surface-muted">
                    <div class="text-xs uppercase tracking-wider text-accent-500 font-semibold mb-1">
                        <i class="fa-solid fa-arrow-right-arrow-left mr-1"></i>{n.edge.relation}
                    </div>
                    <div class="text-sm">
                        {n.node.label}
                        <span class="text-text-muted text-[11px]">({n.node.node_type})</span>
                    </div>
                </div>
            {/each}
        {:else if nodes.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">No knowledge nodes</p>
        {:else}
            {#each nodes as n (n.id)}
                <button
                    onclick={() => viewNode(n.id)}
                    class="w-full text-left p-3 border border-border rounded-md mb-2 bg-surface-muted cursor-pointer hover:border-primary-500/50 transition-colors"
                >
                    <div class="text-xs uppercase tracking-wider text-primary-500 font-semibold mb-1">
                        <i class="fa-solid fa-tag mr-1"></i>{n.node_type ?? 'node'}
                    </div>
                    <div class="text-sm font-semibold mb-1">{n.label}</div>
                    <div class="text-xs text-text-muted">{n.content.slice(0, 200)}</div>
                    <div class="text-[11px] text-text-subtle mt-1">
                        confidence: {(n.confidence ?? 1).toFixed(2)} &middot; {formatDateTime(n.updated_at)}
                    </div>
                </button>
            {/each}
        {/if}
    </div>
</section>
