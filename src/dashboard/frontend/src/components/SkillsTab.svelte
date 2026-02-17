<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import type { SkillStatus } from '../lib/types';
    import SkillCard from './SkillCard.svelte';

    interface OAuthAccount {
        account: string;
        email: string;
        scopes: string[];
        expires_at: string | null;
        updated_at: string | null;
        has_refresh_token: boolean;
    }

    interface ProviderStatus {
        id: string;
        name: string;
        icon: string;
        configured: boolean;
        authorize_url: string;
        accounts: OAuthAccount[];
    }

    interface AllOAuthStatus {
        providers: ProviderStatus[];
    }

    interface SkillExtInfo {
        skill_name: string;
        route_count: number;
        routes: string[];
        ui: {
            panel?: string | null;
            page?: string | null;
            style?: string | null;
            script?: string | null;
            widget?: string | null;
        };
    }

    let skills = $state<SkillStatus[]>([]);
    let error = $state(false);
    let oauth = $state<AllOAuthStatus | null>(null);
    let extensions = $state<SkillExtInfo[]>([]);
    let refreshing = $state<string | null>(null);
    let expandedProviders = $state<Set<string>>(new Set());

    async function load() {
        error = false;
        try {
            skills = await api<SkillStatus[]>('GET', '/api/skills');
        } catch (e) {
            error = true;
            console.error('loadSkills:', e);
        }
    }

    async function loadExtensions() {
        try {
            extensions = await api<SkillExtInfo[]>('GET', '/api/skills/extensions');
        } catch (e) {
            console.error('loadExtensions:', e);
        }
    }

    function getExtension(skillName: string): SkillExtInfo | null {
        return extensions.find(e => e.skill_name === skillName) ?? null;
    }

    async function loadOAuth() {
        try {
            oauth = await api<AllOAuthStatus>('GET', '/api/oauth/status');
        } catch (e) {
            console.error('loadOAuth:', e);
        }
    }

    async function refreshToken(providerId: string, account?: string) {
        refreshing = account ?? `${providerId}:all`;
        try {
            const path = account
                ? `/api/oauth/${providerId}/refresh?account=${encodeURIComponent(account)}`
                : `/api/oauth/${providerId}/refresh`;
            await api('POST', path);
            await loadOAuth();
        } catch (e) {
            console.error('refreshToken:', e);
        } finally {
            refreshing = null;
        }
    }

    async function disconnect(providerId: string, account: string, email: string) {
        if (!confirm(`Disconnect ${email} from ${providerId}?`)) return;
        try {
            await api('POST', `/api/oauth/${providerId}/disconnect/${encodeURIComponent(account)}`);
            await loadOAuth();
        } catch (e) {
            console.error('disconnect:', e);
        }
    }

    function toggleProvider(id: string) {
        const next = new Set(expandedProviders);
        if (next.has(id)) next.delete(id); else next.add(id);
        expandedProviders = next;
    }

    function connectedProviders(providers: ProviderStatus[]) {
        return providers.filter(p => p.accounts.length > 0);
    }

    function availableProviders(providers: ProviderStatus[]) {
        return providers.filter(p => p.accounts.length === 0);
    }

    $effect(() => {
        if (dashboard.currentTab === 'skills') {
            dashboard.refreshCounter;
            load();
            loadOAuth();
            loadExtensions();
        }
    });
</script>

<!-- OAuth Connections -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden mb-4">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-link mr-1.5"></i> Connected Accounts
        </h2>
        {#if oauth}
            <span class="text-xs text-text-muted pr-3">
                {connectedProviders(oauth.providers).reduce((n, p) => n + p.accounts.length, 0)} account{connectedProviders(oauth.providers).reduce((n, p) => n + p.accounts.length, 0) !== 1 ? 's' : ''}
            </span>
        {/if}
    </div>
    <div class="p-3">
        {#if oauth === null}
            <p class="text-text-subtle text-sm italic text-center py-2">Loading...</p>
        {:else}
            <!-- Connected providers -->
            {#each connectedProviders(oauth.providers) as provider (provider.id)}
                <div class="mb-3 last:mb-0">
                    <div
                        onclick={() => toggleProvider(provider.id)}
                        role="button"
                        tabindex="0"
                        onkeydown={(e) => { if (e.key === 'Enter') toggleProvider(provider.id); }}
                        class="w-full flex items-center justify-between p-2.5 rounded-lg bg-surface-elevated border border-border hover:border-text-subtle/30 transition-colors cursor-pointer"
                    >
                        <div class="flex items-center gap-2">
                            <i class="{provider.icon} text-sm w-5 text-center"></i>
                            <span class="text-sm font-medium text-text">{provider.name}</span>
                            <span class="text-xs px-1.5 py-0.5 rounded-full bg-green-900/40 text-green-400 border border-green-800/50">
                                {provider.accounts.length}
                            </span>
                        </div>
                        <div class="flex items-center gap-2">
                            {#if provider.accounts.length > 1}
                                <button
                                    onclick={(e) => { e.stopPropagation(); refreshToken(provider.id); }}
                                    disabled={refreshing !== null}
                                    class="px-2 py-0.5 text-xs border border-border rounded bg-surface hover:bg-surface-elevated transition-colors disabled:opacity-50"
                                    title="Refresh all"
                                >
                                    <i class="fa-solid fa-arrows-rotate" class:fa-spin={refreshing === `${provider.id}:all`}></i>
                                </button>
                            {/if}
                            <a
                                href={provider.authorize_url}
                                onclick={(e) => e.stopPropagation()}
                                class="px-2 py-0.5 text-xs border border-border rounded bg-surface hover:bg-surface-elevated transition-colors text-text-muted"
                                title="Add account"
                            >
                                <i class="fa-solid fa-plus"></i>
                            </a>
                            <i class="fa-solid fa-chevron-{expandedProviders.has(provider.id) ? 'up' : 'down'} text-xs text-text-subtle"></i>
                        </div>
                    </div>

                    {#if expandedProviders.has(provider.id) || provider.accounts.length <= 2}
                        <div class="mt-1.5 space-y-1.5 pl-7">
                            {#each provider.accounts as acct (acct.account)}
                                <div class="flex items-center justify-between p-2 rounded-md bg-surface border border-border/50">
                                    <div class="min-w-0 flex-1 space-y-0.5">
                                        <div class="text-sm text-text truncate">{acct.email}</div>
                                        <div class="flex items-center gap-2 text-xs text-text-subtle">
                                            {#if acct.updated_at}
                                                <span>Updated: {acct.updated_at}</span>
                                            {/if}
                                            {#if !acct.has_refresh_token}
                                                <span class="text-amber-400">
                                                    <i class="fa-solid fa-triangle-exclamation"></i> No refresh token
                                                </span>
                                            {/if}
                                        </div>
                                    </div>
                                    <div class="flex gap-1.5 ml-2 flex-shrink-0">
                                        <button
                                            onclick={() => refreshToken(provider.id, acct.account)}
                                            disabled={refreshing !== null}
                                            class="px-2 py-1 text-xs border border-border rounded bg-surface hover:bg-surface-elevated transition-colors disabled:opacity-50"
                                            title="Refresh"
                                        >
                                            <i class="fa-solid fa-arrows-rotate" class:fa-spin={refreshing === acct.account}></i>
                                        </button>
                                        <button
                                            onclick={() => disconnect(provider.id, acct.account, acct.email)}
                                            class="px-2 py-1 text-xs border border-red-800/50 rounded bg-red-900/20 text-red-400 hover:bg-red-900/40 transition-colors"
                                            title="Disconnect"
                                        >
                                            <i class="fa-solid fa-xmark"></i>
                                        </button>
                                    </div>
                                </div>
                            {/each}
                        </div>
                    {/if}
                </div>
            {/each}

            <!-- Available providers (not connected yet) -->
            {#if availableProviders(oauth.providers).length > 0}
                <div class="mt-3 pt-3 border-t border-border/50">
                    <p class="text-xs text-text-subtle mb-2 uppercase tracking-wider">Available Integrations</p>
                    <div class="flex flex-wrap gap-2">
                        {#each availableProviders(oauth.providers) as provider (provider.id)}
                            {#if provider.configured}
                                <a
                                    href={provider.authorize_url}
                                    class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-border bg-surface hover:bg-surface-elevated hover:border-text-subtle/30 transition-colors text-text"
                                >
                                    <i class="{provider.icon}"></i> {provider.name}
                                </a>
                            {:else}
                                <span
                                    class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md border border-border/50 bg-surface text-text-subtle opacity-50 cursor-not-allowed"
                                    title="Not configured — set {provider.id.toUpperCase()}_CLIENT_ID and {provider.id.toUpperCase()}_CLIENT_SECRET"
                                >
                                    <i class="{provider.icon}"></i> {provider.name}
                                </span>
                            {/if}
                        {/each}
                    </div>
                </div>
            {/if}

            {#if oauth.providers.length === 0}
                <p class="text-text-subtle text-sm italic text-center py-2">No OAuth providers available.</p>
            {/if}
        {/if}
    </div>
</section>

<!-- Skills List -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-puzzle-piece mr-1.5"></i> Skills &amp; Credentials
        </h2>
        <div class="flex items-center gap-2 pr-3">
            <span class="text-xs text-text-muted">
                {skills.length} skill{skills.length !== 1 ? 's' : ''}
            </span>
            <button
                onclick={load}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-arrows-rotate mr-1"></i> Refresh
            </button>
        </div>
    </div>
    <div class="p-3">
        {#if error}
            <p class="text-text-subtle text-sm italic text-center py-4">Error loading skills</p>
        {:else if skills.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">
                No skills installed. Skills will appear here when the agent creates them.
            </p>
        {:else}
            {#each skills as skill (skill.name)}
                <SkillCard {skill} onrefresh={load} extension={getExtension(skill.name)} />
            {/each}
        {/if}
    </div>
</section>
