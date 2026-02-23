<script lang="ts">
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import { dashboard, auth, refreshAll, toggleTheme, requestNotifications } from '../lib/state.svelte';
    import type { AgentStatus, ActionResponse } from '../lib/types';

    let paused = $state(false);
    let toolsCount = $state(0);
    let statusClass = $state('');
    let statusIcon = $state('fa-spinner fa-spin');
    let statusText = $state(t('header.loading'));
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
                statusText = t('header.paused');
            } else {
                statusClass = 'badge-running';
                statusIcon = 'fa-circle-check';
                statusText = t('header.running');
            }
        } catch {
            disconnected = true;
            statusClass = 'text-error-500';
            statusIcon = 'fa-circle-xmark';
            statusText = t('header.disconnected');
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

<header class="flex justify-between items-center px-3 sm:px-6 py-3 sm:py-4 border-b border-border bg-surface">
    <div class="flex items-center gap-2 sm:gap-3">
        <!-- Mobile hamburger -->
        <button
            class="sm:hidden p-2 -ml-1 text-text-muted hover:text-text transition-colors"
            onclick={() => dashboard.mobileMenuOpen = true}
            title={t('header.menu')}
        >
            <i class="fa-solid fa-bars text-lg"></i>
        </button>

        <h1 class="text-base sm:text-lg font-semibold tracking-tight text-primary-400">
            <i class="fa-solid fa-robot mr-1"></i>
            <span class="hidden xs:inline">{t('header.title')}</span>
        </h1>
        <span class="text-xs px-2 py-0.5 rounded-full font-medium {statusClass}">
            <i class="fa-solid {statusIcon} mr-1"></i><span class="hidden sm:inline">{statusText}</span>
        </span>
        <span class="hidden md:inline-flex text-xs px-2 py-0.5 rounded-full font-medium bg-primary-950 text-primary-400">
            <i class="fa-solid fa-screwdriver-wrench mr-1"></i>{t('header.tools_count', { count: toolsCount })}
        </span>
    </div>
    <div class="flex items-center gap-1.5 sm:gap-2">
        <!-- Theme toggle -->
        <button
            onclick={toggleTheme}
            class="p-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors text-text-muted"
            title={dashboard.theme === 'dark' ? t('header.theme_light') : t('header.theme_dark')}
        >
            <i class="fa-solid {dashboard.theme === 'dark' ? 'fa-sun' : 'fa-moon'}"></i>
        </button>

        <!-- Notification toggle -->
        <button
            onclick={requestNotifications}
            class="p-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors"
            class:text-primary-400={dashboard.notificationsEnabled}
            class:text-text-muted={!dashboard.notificationsEnabled}
            title={dashboard.notificationsEnabled ? t('notifications.enabled') : t('notifications.enable')}
        >
            <i class="fa-solid {dashboard.notificationsEnabled ? 'fa-bell' : 'fa-bell-slash'}"></i>
        </button>

        {#if paused}
            <button
                onclick={resume}
                class="hidden sm:inline-flex px-4 py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-play mr-1"></i> {t('header.resume')}
            </button>
        {:else}
            <button
                onclick={pause}
                class="hidden sm:inline-flex px-4 py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-pause mr-1"></i> {t('header.pause')}
            </button>
        {/if}

        <button
            onclick={forceTick}
            class="hidden sm:inline-flex px-4 py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors"
        >
            <i class="fa-solid fa-bolt mr-1"></i> <span class="hidden lg:inline">{t('header.force_tick')}</span>
        </button>

        {#if auth.subject}
            <span class="hidden lg:inline-flex text-xs text-text-muted border border-border rounded px-2 py-1">
                <i class="fa-solid fa-user mr-1"></i>
                {auth.subject}
                {#if auth.role}
                    <span class="text-text-muted/60 ml-1">({auth.role})</span>
                {/if}
            </span>
        {/if}

        <button
            onclick={logout}
            class="p-2 sm:px-4 sm:py-2 border border-border rounded-md bg-surface text-sm hover:bg-surface-elevated transition-colors text-text-muted"
            title={t('header.sign_out')}
        >
            <i class="fa-solid fa-right-from-bracket"></i>
        </button>
    </div>
</header>
