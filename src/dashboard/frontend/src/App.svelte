<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import { dashboard, auth, refreshAll } from './lib/state.svelte';
    import LoginOverlay from './components/LoginOverlay.svelte';
    import Header from './components/Header.svelte';
    import PendingActions from './components/PendingActions.svelte';
    import ActivityLog from './components/ActivityLog.svelte';
    import MemoryPanel from './components/MemoryPanel.svelte';
    import StatsPanel from './components/StatsPanel.svelte';
    import ChatTab from './components/ChatTab.svelte';
    import SkillsTab from './components/SkillsTab.svelte';
    import KnowledgeTab from './components/KnowledgeTab.svelte';
    import ToolsTab from './components/ToolsTab.svelte';

    const POLL_INTERVAL_MS = 30_000;

    const tabs = [
        { id: 'overview' as const, label: 'Overview', icon: 'fa-chart-line' },
        { id: 'chat' as const, label: 'Chat', icon: 'fa-comments' },
        { id: 'skills' as const, label: 'Skills', icon: 'fa-puzzle-piece' },
        { id: 'knowledge' as const, label: 'Knowledge', icon: 'fa-diagram-project' },
        { id: 'tools' as const, label: 'Tools', icon: 'fa-screwdriver-wrench' },
    ];

    function switchTab(id: typeof dashboard.currentTab) {
        dashboard.currentTab = id;
    }

    async function checkAuth() {
        try {
            const res = await fetch('/api/auth/check');
            const data = await res.json();
            auth.authenticated = data.authenticated === true;
        } catch {
            auth.authenticated = false;
        } finally {
            auth.checked = true;
        }
    }

    onMount(() => {
        checkAuth();
    });

    let evtSource: EventSource | undefined;
    let interval: ReturnType<typeof setInterval> | undefined;

    $effect(() => {
        if (auth.authenticated) {
            // untrack so the refreshCounter++ read doesn't become a dependency
            untrack(() => refreshAll());

            evtSource = new EventSource('/api/events');
            evtSource.onmessage = () => refreshAll();
            evtSource.onerror = () => console.error('SSE connection lost, will retry...');

            interval = setInterval(refreshAll, POLL_INTERVAL_MS);
        }

        return () => {
            evtSource?.close();
            evtSource = undefined;
            if (interval) {
                clearInterval(interval);
                interval = undefined;
            }
        };
    });
</script>

{#if !auth.checked}
    <!-- Loading: waiting for auth check -->
{:else if !auth.authenticated}
    <LoginOverlay />
{:else}
    <Header />

    <main class="p-6">
        <div class="flex border-b border-border mb-4">
            {#each tabs as tab}
                <button
                    class="main-tab"
                    class:active={dashboard.currentTab === tab.id}
                    onclick={() => switchTab(tab.id)}
                >
                    <i class="fa-solid {tab.icon} mr-1.5"></i> {tab.label}
                </button>
            {/each}
        </div>

        {#if dashboard.currentTab === 'overview'}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                <PendingActions />
                <ActivityLog />
                <MemoryPanel />
                <StatsPanel />
            </div>
        {:else if dashboard.currentTab === 'chat'}
            <ChatTab />
        {:else if dashboard.currentTab === 'skills'}
            <SkillsTab />
        {:else if dashboard.currentTab === 'knowledge'}
            <KnowledgeTab />
        {:else if dashboard.currentTab === 'tools'}
            <ToolsTab />
        {/if}
    </main>
{/if}
