<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard, auth, refreshAll } from '../lib/state.svelte';
    import type { AgentStatus, ActionResponse } from '../lib/types';

    let paused = $state(false);
    let toolsCount = $state(0);
    let statusClass = $state('');
    let statusIcon = $state('fa-spinner fa-spin');
    let statusText = $state('loading...');
    let disconnected = $state(false);

    async function loadStatus() {
        try {
            const status = await api<AgentStatus>('GET', '/api/status');
            paused = status.paused;
            toolsCount = status.tools_count;
            disconnected = false;

            if (status.paused) {
                statusClass = 'badge-paused';
                statusIcon = 'fa-circle-pause';
                statusText = 'paused';
            } else {
                statusClass = 'badge-running';
                statusIcon = 'fa-circle-check';
                statusText = 'running';
            }
        } catch {
            disconnected = true;
            statusClass = 'text-error-500';
            statusIcon = 'fa-circle-xmark';
            statusText = 'disconnected';
        }
    }

    async function pause() {
        await api<ActionResponse>('POST', '/api/agent/pause');
        refreshAll();
    }

    async function resume() {
        await api<ActionResponse>('POST', '/api/agent/resume');
        refreshAll();
    }

    async function forceTick() {
        await api<ActionResponse>('POST', '/api/agent/tick');
        refreshAll();
    }

    async function logout() {
        try {
            await fetch('/api/auth/logout', { method: 'POST' });
        } catch { /* ignore */ }
        auth.authenticated = false;
    }

    $effect(() => {
        dashboard.refreshCounter;
        loadStatus();
    });
</script>

<header class="flex justify-between items-center px-6 py-4 border-b border-border bg-surface">
    <div class="flex items-center gap-3">
        <h1 class="text-lg font-semibold tracking-tight text-primary-400">
            <i class="fa-solid fa-robot mr-1"></i> safe-agent
        </h1>
        <span class="text-xs px-2 py-0.5 rounded-full font-medium {statusClass}">
            <i class="fa-solid {statusIcon} mr-1"></i>{statusText}
        </span>
        <span class="text-xs px-2 py-0.5 rounded-full font-medium bg-primary-950 text-primary-400">
            <i class="fa-solid fa-screwdriver-wrench mr-1"></i>{toolsCount} tools
        </span>
    </div>
    <div class="flex gap-2">
        {#if paused}
            <button
                onclick={resume}
                class="px-4 py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-play mr-1"></i> Resume
            </button>
        {:else}
            <button
                onclick={pause}
                class="px-4 py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-pause mr-1"></i> Pause
            </button>
        {/if}

        <button
            onclick={forceTick}
            class="px-4 py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors"
        >
            <i class="fa-solid fa-bolt mr-1"></i> Force Tick
        </button>

        <button
            onclick={logout}
            class="px-4 py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors text-text-muted"
            title="Sign out"
        >
            <i class="fa-solid fa-right-from-bracket"></i>
        </button>
    </div>
</header>
