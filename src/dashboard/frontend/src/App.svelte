<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import { dashboard, auth, refreshAll, pushToolEvent, clearLiveFeed } from './lib/state.svelte';
    import type { ToolEvent } from './lib/types';
    import LoginOverlay from './components/LoginOverlay.svelte';
    import OnboardingWizard from './components/OnboardingWizard.svelte';
    import Header from './components/Header.svelte';
    import PendingActions from './components/PendingActions.svelte';
    import ActivityLog from './components/ActivityLog.svelte';
    import LiveFeed from './components/LiveFeed.svelte';
    import MemoryPanel from './components/MemoryPanel.svelte';
    import StatsPanel from './components/StatsPanel.svelte';
    import ChatTab from './components/ChatTab.svelte';
    import GoalsTab from './components/GoalsTab.svelte';
    import SkillsTab from './components/SkillsTab.svelte';
    import KnowledgeTab from './components/KnowledgeTab.svelte';
    import ToolsTab from './components/ToolsTab.svelte';
    import TrashTab from './components/TrashTab.svelte';
    import SecurityTab from './components/SecurityTab.svelte';
    import OperationsTab from './components/OperationsTab.svelte';
    import SettingsTab from './components/SettingsTab.svelte';

    const POLL_INTERVAL_MS = 30_000;

    const tabs = [
        { id: 'overview' as const, label: 'Overview', icon: 'fa-chart-line' },
        { id: 'chat' as const, label: 'Chat', icon: 'fa-comments' },
        { id: 'goals' as const, label: 'Goals', icon: 'fa-bullseye' },
        { id: 'skills' as const, label: 'Skills', icon: 'fa-puzzle-piece' },
        { id: 'knowledge' as const, label: 'Knowledge', icon: 'fa-diagram-project' },
        { id: 'tools' as const, label: 'Tools', icon: 'fa-screwdriver-wrench' },
        { id: 'trash' as const, label: 'Trash', icon: 'fa-trash-can' },
        { id: 'security' as const, label: 'Security', icon: 'fa-shield-halved' },
        { id: 'operations' as const, label: 'Ops', icon: 'fa-server' },
        { id: 'settings' as const, label: 'Settings', icon: 'fa-gear' },
    ];

    function switchTab(id: typeof dashboard.currentTab) {
        dashboard.currentTab = id;
    }

    async function checkAuth() {
        try {
            const res = await fetch('/api/auth/check');
            const data = await res.json();
            auth.authenticated = data.authenticated === true;
            auth.userId = data.user_id || null;
            auth.role = data.role || null;
            auth.subject = data.subject || null;
        } catch {
            auth.authenticated = false;
        } finally {
            auth.checked = true;
        }
    }

    async function checkOnboarding() {
        try {
            const res = await fetch('/api/onboarding/status');
            const data = await res.json();
            auth.onboardingCompleted = data.completed === true;
        } catch {
            // If the endpoint fails, assume onboarding is done so we don't block
            auth.onboardingCompleted = true;
        }
    }

    function handleOnboardingComplete() {
        auth.onboardingCompleted = true;
    }

    onMount(() => {
        checkOnboarding();
        checkAuth();
        window.addEventListener('onboarding-complete', handleOnboardingComplete);
        return () => {
            window.removeEventListener('onboarding-complete', handleOnboardingComplete);
        };
    });

    let evtSource: EventSource | undefined;
    let interval: ReturnType<typeof setInterval> | undefined;

    $effect(() => {
        if (auth.authenticated) {
            // untrack so the refreshCounter++ read doesn't become a dependency
            untrack(() => refreshAll());

            evtSource = new EventSource('/api/events');
            evtSource.onmessage = (e) => {
                const data = e.data;
                // Try to parse as a structured tool event
                if (data && data.startsWith('{')) {
                    try {
                        const evt = JSON.parse(data) as ToolEvent;
                        if (evt.type) {
                            pushToolEvent(evt);
                            // Also trigger a data refresh on turn_complete
                            if (evt.type === 'turn_complete' || evt.type === 'approval_needed') {
                                refreshAll();
                            }
                            return;
                        }
                    } catch { /* not JSON, fall through */ }
                }
                // Backward-compatible: plain "update" string
                refreshAll();
            };
            evtSource.onerror = () => {
                console.error('SSE connection lost, will retry...');
                clearLiveFeed();
            };

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
{:else if auth.onboardingCompleted === false}
    <OnboardingWizard />
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
            <LiveFeed />
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
                <PendingActions />
                <ActivityLog />
                <MemoryPanel />
                <StatsPanel />
            </div>
        {:else if dashboard.currentTab === 'chat'}
            <ChatTab />
        {:else if dashboard.currentTab === 'goals'}
            <GoalsTab />
        {:else if dashboard.currentTab === 'skills'}
            <SkillsTab />
        {:else if dashboard.currentTab === 'knowledge'}
            <KnowledgeTab />
        {:else if dashboard.currentTab === 'tools'}
            <ToolsTab />
        {:else if dashboard.currentTab === 'trash'}
            <TrashTab />
        {:else if dashboard.currentTab === 'security'}
            <SecurityTab />
        {:else if dashboard.currentTab === 'operations'}
            <OperationsTab />
        {:else if dashboard.currentTab === 'settings'}
            <SettingsTab />
        {/if}
    </main>
{/if}
