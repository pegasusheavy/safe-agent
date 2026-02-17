<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard, auth } from '../lib/state.svelte';
    import { formatDateTime } from '../lib/time';
    import UsersPanel from './UsersPanel.svelte';
    import TwoFactorPanel from './TwoFactorPanel.svelte';

    interface SsoProvider {
        id: string;
        name: string;
        icon: string;
        login_url: string;
    }

    interface AuthInfo {
        password_enabled: boolean;
        sso_providers: SsoProvider[];
    }

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

    interface TelegramConfigInfo {
        enabled: boolean;
        connected: boolean;
        has_token: boolean;
        allowed_chat_ids: number[];
        primary_channel: string | null;
    }

    interface WhatsAppConfigInfo {
        enabled: boolean;
        connected: boolean;
        bridge_port: number;
        webhook_port: number;
        allowed_numbers: string[];
        primary_channel: string | null;
        bridge_status: string;
        qr: string | null;
        connected_number: string | null;
    }

    interface MessagingConfig {
        telegram: TelegramConfigInfo;
        whatsapp: WhatsAppConfigInfo;
        active_platforms: string[];
    }

    let msgConfig = $state<MessagingConfig | null>(null);
    let oauth = $state<AllOAuthStatus | null>(null);
    let authInfo = $state<AuthInfo | null>(null);
    let refreshing = $state<string | null>(null);
    let expandedProviders = $state<Set<string>>(new Set());
    let waQrImage = $state<string | null>(null);

    async function loadConfig() {
        try {
            msgConfig = await api<MessagingConfig>('GET', '/api/messaging/config');
            if (msgConfig?.whatsapp?.bridge_status === 'pairing' && msgConfig?.whatsapp?.qr) {
                try {
                    const qrResp = await api<{ qr: string }>('GET', '/api/messaging/whatsapp/qr');
                    waQrImage = qrResp?.qr ?? null;
                } catch {
                    waQrImage = null;
                }
            } else {
                waQrImage = null;
            }
        } catch (e) {
            console.error('loadConfig:', e);
        }
    }

    async function loadOAuth() {
        try {
            oauth = await api<AllOAuthStatus>('GET', '/api/oauth/status');
        } catch (e) {
            console.error('loadOAuth:', e);
        }
    }

    async function loadAuth() {
        try {
            authInfo = await api<AuthInfo>('GET', '/api/auth/info');
        } catch (e) {
            console.error('loadAuth:', e);
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

    // Timezone & Locale state
    let tzInfo = $state<{
        system_timezone: string;
        system_locale: string;
        user_timezone: string;
        user_locale: string;
        effective_timezone: string;
        effective_locale: string;
        current_time_formatted: string;
    } | null>(null);
    let allTimezones = $state<string[]>([]);
    let selectedTimezone = $state('');
    let selectedLocale = $state('');
    let tzSaving = $state(false);
    let tzMessage = $state('');
    let tzFilter = $state('');

    async function loadTimezone() {
        try {
            const uid = auth.userId || '';
            const data = await api<any>('GET', `/api/timezone?user_id=${encodeURIComponent(uid)}`);
            tzInfo = data;
            selectedTimezone = data?.user_timezone || '';
            selectedLocale = data?.user_locale || '';
        } catch (e) {
            console.error('loadTimezone:', e);
        }
    }

    async function loadTimezoneList() {
        if (allTimezones.length > 0) return;
        try {
            const data = await api<{ timezones: string[] }>('GET', '/api/timezones');
            allTimezones = data?.timezones ?? [];
        } catch (e) {
            console.error('loadTimezoneList:', e);
        }
    }

    async function saveTimezone() {
        if (!auth.userId) return;
        tzSaving = true;
        tzMessage = '';
        try {
            const data = await api<{ ok: boolean; message?: string }>('POST', '/api/timezone', {
                user_id: auth.userId,
                timezone: selectedTimezone || undefined,
                locale: selectedLocale || undefined,
            });
            tzMessage = data?.ok ? 'Saved' : (data?.message ?? 'Failed');
            await loadTimezone();
            setTimeout(() => { tzMessage = ''; }, 3000);
        } catch (e) {
            tzMessage = 'Network error';
        } finally {
            tzSaving = false;
        }
    }

    function detectBrowserTimezone() {
        try {
            selectedTimezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
        } catch {
            selectedTimezone = 'UTC';
        }
    }

    function detectBrowserLocale() {
        try {
            selectedLocale = navigator.language || 'en-US';
        } catch {
            selectedLocale = 'en-US';
        }
    }

    const commonTimezones = [
        'UTC',
        'America/New_York', 'America/Chicago', 'America/Denver', 'America/Los_Angeles',
        'America/Toronto', 'America/Vancouver', 'America/Sao_Paulo', 'America/Mexico_City',
        'Europe/London', 'Europe/Paris', 'Europe/Berlin', 'Europe/Moscow',
        'Asia/Tokyo', 'Asia/Shanghai', 'Asia/Kolkata', 'Asia/Singapore', 'Asia/Dubai',
        'Australia/Sydney', 'Australia/Melbourne', 'Pacific/Auckland',
        'Africa/Cairo', 'Africa/Johannesburg',
    ];

    $effect(() => {
        if (dashboard.currentTab === 'settings') {
            dashboard.refreshCounter;
            loadConfig();
            loadOAuth();
            loadAuth();
            loadTimezone();
            loadTimezoneList();
        }
    });
</script>

<!-- Messaging Platforms -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden mb-4">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-tower-broadcast mr-1.5"></i> Messaging Platforms
        </h2>
        <button
            onclick={loadConfig}
            class="mr-3 px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
        >
            <i class="fa-solid fa-arrows-rotate mr-1"></i> Refresh
        </button>
    </div>
    <div class="p-4 space-y-4">
        {#if !msgConfig}
            <p class="text-text-subtle text-sm italic text-center py-2">Loading...</p>
        {:else}
            <!-- Telegram -->
            <div class="rounded-lg border border-border overflow-hidden">
                <div class="flex items-center justify-between p-3 bg-surface-elevated">
                    <div class="flex items-center gap-2.5">
                        <i class="fa-brands fa-telegram text-blue-400 text-lg"></i>
                        <div>
                            <span class="text-sm font-medium text-text">Telegram</span>
                            <p class="text-xs text-text-subtle mt-0.5">Long-polling bot via teloxide</p>
                        </div>
                    </div>
                    {#if msgConfig.telegram.enabled && msgConfig.telegram.connected}
                        <span class="text-xs px-2 py-0.5 rounded-full border bg-green-900/40 text-green-400 border-green-800/50">Connected</span>
                    {:else if msgConfig.telegram.enabled}
                        <span class="text-xs px-2 py-0.5 rounded-full border bg-red-900/40 text-red-400 border-red-800/50">Disconnected</span>
                    {:else}
                        <span class="text-xs px-2 py-0.5 rounded-full border bg-zinc-800/60 text-text-subtle border-border">Disabled</span>
                    {/if}
                </div>
                <div class="p-3 space-y-2 text-sm">
                    <div class="flex justify-between">
                        <span class="text-text-muted">Enabled</span>
                        <span class="text-text">{msgConfig.telegram.enabled ? 'Yes' : 'No'}</span>
                    </div>
                    <div class="flex justify-between">
                        <span class="text-text-muted">Bot Token</span>
                        <span class="{msgConfig.telegram.has_token ? 'text-green-400' : 'text-red-400'}">
                            {msgConfig.telegram.has_token ? 'Set (TELEGRAM_BOT_TOKEN)' : 'Not set'}
                        </span>
                    </div>
                    {#if msgConfig.telegram.allowed_chat_ids.length > 0}
                        <div class="flex justify-between">
                            <span class="text-text-muted">Allowed Chat IDs</span>
                            <span class="text-text font-mono text-xs">
                                {msgConfig.telegram.allowed_chat_ids.join(', ')}
                            </span>
                        </div>
                    {:else}
                        <div class="flex justify-between">
                            <span class="text-text-muted">Allowed Chat IDs</span>
                            <span class="text-amber-400 text-xs">
                                <i class="fa-solid fa-triangle-exclamation mr-1"></i> None (all denied)
                            </span>
                        </div>
                    {/if}
                    {#if msgConfig.telegram.primary_channel}
                        <div class="flex justify-between">
                            <span class="text-text-muted">Primary Channel</span>
                            <span class="text-text font-mono text-xs">{msgConfig.telegram.primary_channel}</span>
                        </div>
                    {/if}
                    {#if !msgConfig.telegram.enabled}
                        <div class="mt-2 p-2 rounded bg-zinc-800/40 border border-border/50">
                            <p class="text-xs text-text-subtle">
                                <i class="fa-solid fa-circle-info mr-1"></i>
                                Enable Telegram by setting <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">telegram.enabled = true</code> in your config and providing a <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">TELEGRAM_BOT_TOKEN</code> env var.
                            </p>
                        </div>
                    {/if}
                </div>
            </div>

            <!-- WhatsApp -->
            <div class="rounded-lg border border-border overflow-hidden">
                <div class="flex items-center justify-between p-3 bg-surface-elevated">
                    <div class="flex items-center gap-2.5">
                        <i class="fa-brands fa-whatsapp text-green-400 text-lg"></i>
                        <div>
                            <span class="text-sm font-medium text-text">WhatsApp</span>
                            <p class="text-xs text-text-subtle mt-0.5">Baileys bridge (Node.js)</p>
                        </div>
                    </div>
                    {#if !msgConfig.whatsapp.enabled}
                        <span class="text-xs px-2 py-0.5 rounded-full border bg-zinc-800/60 text-text-subtle border-border">Disabled</span>
                    {:else if msgConfig.whatsapp.bridge_status === 'connected'}
                        <span class="text-xs px-2 py-0.5 rounded-full border bg-green-900/40 text-green-400 border-green-800/50">Connected</span>
                    {:else if msgConfig.whatsapp.bridge_status === 'pairing'}
                        <span class="text-xs px-2 py-0.5 rounded-full border bg-amber-900/40 text-amber-400 border-amber-800/50">Pairing</span>
                    {:else}
                        <span class="text-xs px-2 py-0.5 rounded-full border bg-red-900/40 text-red-400 border-red-800/50">{msgConfig.whatsapp.bridge_status}</span>
                    {/if}
                </div>
                <div class="p-3 space-y-2 text-sm">
                    <div class="flex justify-between">
                        <span class="text-text-muted">Enabled</span>
                        <span class="text-text">{msgConfig.whatsapp.enabled ? 'Yes' : 'No'}</span>
                    </div>
                    {#if msgConfig.whatsapp.enabled}
                        <div class="flex justify-between">
                            <span class="text-text-muted">Bridge Port</span>
                            <span class="text-text font-mono text-xs">{msgConfig.whatsapp.bridge_port}</span>
                        </div>
                        <div class="flex justify-between">
                            <span class="text-text-muted">Webhook Port</span>
                            <span class="text-text font-mono text-xs">{msgConfig.whatsapp.webhook_port}</span>
                        </div>
                        {#if msgConfig.whatsapp.allowed_numbers.length > 0}
                            <div class="flex justify-between">
                                <span class="text-text-muted">Allowed Numbers</span>
                                <span class="text-text font-mono text-xs">
                                    {msgConfig.whatsapp.allowed_numbers.join(', ')}
                                </span>
                            </div>
                        {:else}
                            <div class="flex justify-between">
                                <span class="text-text-muted">Allowed Numbers</span>
                                <span class="text-amber-400 text-xs">
                                    <i class="fa-solid fa-triangle-exclamation mr-1"></i> None (all denied)
                                </span>
                            </div>
                        {/if}
                        {#if msgConfig.whatsapp.connected_number}
                            <div class="flex justify-between">
                                <span class="text-text-muted">Connected Number</span>
                                <span class="text-green-400 font-mono text-xs">{msgConfig.whatsapp.connected_number}</span>
                            </div>
                        {/if}

                        <!-- QR Code (if pairing) -->
                        {#if msgConfig.whatsapp.bridge_status === 'pairing'}
                            <div class="mt-3 p-4 rounded-lg bg-surface border border-amber-800/50 text-center">
                                <p class="text-sm text-amber-400 mb-3">
                                    <i class="fa-solid fa-qrcode mr-1.5"></i> Scan this QR code with WhatsApp to pair
                                </p>
                                {#if waQrImage}
                                    <img src={waQrImage} alt="WhatsApp QR Code" class="mx-auto w-52 h-52 rounded-lg shadow-lg" />
                                {:else}
                                    <div class="w-52 h-52 mx-auto bg-zinc-800 rounded-lg flex items-center justify-center">
                                        <span class="text-text-subtle text-sm">Waiting for QR...</span>
                                    </div>
                                {/if}
                                <button
                                    onclick={loadConfig}
                                    class="mt-3 px-4 py-1.5 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
                                >
                                    <i class="fa-solid fa-arrows-rotate mr-1"></i> Refresh QR
                                </button>
                            </div>
                        {/if}
                    {:else}
                        <div class="mt-2 p-2 rounded bg-zinc-800/40 border border-border/50">
                            <p class="text-xs text-text-subtle">
                                <i class="fa-solid fa-circle-info mr-1"></i>
                                Enable WhatsApp by setting <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">whatsapp.enabled = true</code> in your config and adding allowed numbers.
                                The Baileys bridge requires Node.js in the container.
                            </p>
                        </div>
                    {/if}
                </div>
            </div>

            <!-- Summary bar -->
            <div class="flex items-center gap-3 p-2.5 rounded-lg bg-surface-elevated border border-border/50">
                <i class="fa-solid fa-circle-info text-text-subtle"></i>
                <span class="text-xs text-text-subtle">
                    {msgConfig.active_platforms.length} active platform{msgConfig.active_platforms.length !== 1 ? 's' : ''}
                    {#if msgConfig.active_platforms.length > 0}
                        ({msgConfig.active_platforms.join(', ')})
                    {/if}
                    — the primary messaging backend is used by the <code class="px-1 py-0.5 bg-zinc-900 rounded">message</code> tool.
                </span>
            </div>
        {/if}
    </div>
</section>

<!-- Authentication / SSO -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden mb-4">
    <div class="border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-shield-halved mr-1.5"></i> Dashboard Authentication
        </h2>
    </div>
    <div class="p-4 space-y-4">
        {#if !authInfo}
            <p class="text-text-subtle text-sm italic text-center py-2">Loading...</p>
        {:else}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                <!-- Password Login -->
                <div class="rounded-lg border border-border overflow-hidden">
                    <div class="flex items-center justify-between p-3 bg-surface-elevated">
                        <div class="flex items-center gap-2.5">
                            <i class="fa-solid fa-key text-amber-400"></i>
                            <span class="text-sm font-medium text-text">Password Login</span>
                        </div>
                        {#if authInfo.password_enabled}
                            <span class="text-xs px-2 py-0.5 rounded-full border bg-green-900/40 text-green-400 border-green-800/50">Enabled</span>
                        {:else}
                            <span class="text-xs px-2 py-0.5 rounded-full border bg-zinc-800/60 text-text-subtle border-border">Disabled</span>
                        {/if}
                    </div>
                    <div class="p-3 text-sm text-text-subtle">
                        {#if authInfo.password_enabled}
                            <p>Password authentication is active via the <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">DASHBOARD_PASSWORD</code> env var.</p>
                        {:else}
                            <p>Password login is disabled. Users must sign in via SSO.</p>
                        {/if}
                    </div>
                </div>

                <!-- SSO Providers -->
                <div class="rounded-lg border border-border overflow-hidden">
                    <div class="flex items-center justify-between p-3 bg-surface-elevated">
                        <div class="flex items-center gap-2.5">
                            <i class="fa-solid fa-right-to-bracket text-primary-400"></i>
                            <span class="text-sm font-medium text-text">SSO Providers</span>
                        </div>
                        {#if authInfo.sso_providers.length > 0}
                            <span class="text-xs px-2 py-0.5 rounded-full border bg-green-900/40 text-green-400 border-green-800/50">
                                {authInfo.sso_providers.length} configured
                            </span>
                        {:else}
                            <span class="text-xs px-2 py-0.5 rounded-full border bg-zinc-800/60 text-text-subtle border-border">None</span>
                        {/if}
                    </div>
                    <div class="p-3">
                        {#if authInfo.sso_providers.length > 0}
                            <div class="space-y-1.5">
                                {#each authInfo.sso_providers as provider}
                                    <div class="flex items-center gap-2 p-2 rounded-md bg-surface border border-border/50">
                                        <i class="{provider.icon} text-sm w-5 text-center"></i>
                                        <span class="text-sm text-text">{provider.name}</span>
                                        <span class="text-xs px-1.5 py-0.5 rounded bg-primary-900/30 text-primary-400 border border-primary-800/30 ml-auto">SSO</span>
                                    </div>
                                {/each}
                            </div>
                        {:else}
                            <p class="text-sm text-text-subtle">
                                No SSO providers configured. Add providers to <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">[dashboard].sso_providers</code> in config.
                            </p>
                        {/if}
                    </div>
                </div>
            </div>

            {#if !authInfo.password_enabled && authInfo.sso_providers.length === 0}
                <div class="p-3 rounded-lg bg-red-900/20 border border-red-800/40">
                    <p class="text-sm text-red-400">
                        <i class="fa-solid fa-triangle-exclamation mr-1"></i>
                        Password login is disabled and no SSO providers are configured. You may be locked out after your session expires.
                    </p>
                </div>
            {/if}

            <div class="p-3 rounded bg-surface-elevated border border-border/50">
                <p class="text-xs text-text-subtle">
                    <i class="fa-solid fa-circle-info mr-1"></i>
                    Configure SSO in your <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">config.toml</code>:
                </p>
                <code class="block mt-2 whitespace-pre-wrap text-xs text-text-subtle leading-relaxed">[dashboard]
password_enabled = true
sso_providers = ["google", "github"]
sso_allowed_emails = ["you@example.com"]</code>
                <p class="text-xs text-text-subtle mt-2">
                    SSO reuses the same OAuth client credentials (e.g. <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">GOOGLE_CLIENT_ID</code>). The SSO callback URL is <code class="px-1 py-0.5 bg-zinc-900 rounded text-text-muted">/api/auth/sso/&#123;provider&#125;/callback</code>.
                </p>
            </div>
        {/if}
    </div>
</section>

<!-- Two-Factor Authentication -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden mb-4">
    <div class="border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-lock mr-1.5"></i> Two-Factor Authentication
        </h2>
    </div>
    <div class="p-4">
        <TwoFactorPanel />
    </div>
</section>

<!-- OAuth Connections -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden mb-4">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-link mr-1.5"></i> Connected Accounts (OAuth)
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
                                                <span>Updated: {formatDateTime(acct.updated_at)}</span>
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

<!-- Timezone & Locale -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden mb-4">
    <div class="border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-clock mr-1.5"></i> Timezone & Locale
        </h2>
    </div>
    <div class="p-4 space-y-4">
        {#if tzInfo}
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
                <div class="p-3 rounded-lg bg-surface-elevated border border-border">
                    <span class="text-text-muted text-xs uppercase tracking-wide">Current Time</span>
                    <p class="text-text font-medium mt-1">{tzInfo.current_time_formatted}</p>
                </div>
                <div class="p-3 rounded-lg bg-surface-elevated border border-border">
                    <span class="text-text-muted text-xs uppercase tracking-wide">Effective Timezone</span>
                    <p class="text-text font-medium mt-1">{tzInfo.effective_timezone}</p>
                    {#if tzInfo.user_timezone}
                        <span class="text-xs text-text-subtle">(per-user override)</span>
                    {:else}
                        <span class="text-xs text-text-subtle">(system default)</span>
                    {/if}
                </div>
            </div>

            <div class="space-y-3">
                <div>
                    <div class="flex items-center gap-2 mb-1">
                        <span class="text-xs font-medium text-text-muted uppercase tracking-wide">Timezone</span>
                        <button
                            onclick={detectBrowserTimezone}
                            class="text-[10px] px-1.5 py-0.5 rounded border border-border bg-surface-elevated text-text-muted hover:text-text transition-colors"
                        >
                            <i class="fa-solid fa-crosshairs mr-0.5"></i> Detect
                        </button>
                    </div>
                    <div class="relative">
                        <input
                            type="text"
                            bind:value={selectedTimezone}
                            list="tz-list"
                            placeholder={tzInfo.system_timezone || 'UTC'}
                            class="w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text text-sm
                                   placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50"
                        />
                        <datalist id="tz-list">
                            {#each commonTimezones as tz}
                                <option value={tz}></option>
                            {/each}
                            {#each allTimezones.filter(tz => !commonTimezones.includes(tz)) as tz}
                                <option value={tz}></option>
                            {/each}
                        </datalist>
                    </div>
                    <p class="text-xs text-text-subtle mt-1">IANA timezone name. Leave empty to use system default ({tzInfo.system_timezone}).</p>
                </div>

                <div>
                    <div class="flex items-center gap-2 mb-1">
                        <span class="text-xs font-medium text-text-muted uppercase tracking-wide">Locale</span>
                        <button
                            onclick={detectBrowserLocale}
                            class="text-[10px] px-1.5 py-0.5 rounded border border-border bg-surface-elevated text-text-muted hover:text-text transition-colors"
                        >
                            <i class="fa-solid fa-crosshairs mr-0.5"></i> Detect
                        </button>
                    </div>
                    <input
                        type="text"
                        bind:value={selectedLocale}
                        placeholder={tzInfo.system_locale || 'en-US'}
                        class="w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text text-sm
                               placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50"
                    />
                    <p class="text-xs text-text-subtle mt-1">BCP 47 locale tag (e.g. en-US, de-DE, ja-JP). Leave empty to use system default ({tzInfo.system_locale}).</p>
                </div>

                <div class="flex items-center gap-3">
                    <button
                        onclick={saveTimezone}
                        disabled={tzSaving}
                        class="px-4 py-2 rounded-md bg-primary-600 text-white font-medium text-sm
                               hover:bg-primary-500 transition-colors disabled:opacity-50"
                    >
                        {#if tzSaving}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> Saving...
                        {:else}
                            <i class="fa-solid fa-floppy-disk mr-1"></i> Save
                        {/if}
                    </button>
                    {#if tzMessage}
                        <span class="text-xs {tzMessage === 'Saved' ? 'text-green-400' : 'text-red-400'}">{tzMessage}</span>
                    {/if}
                </div>
            </div>
        {:else}
            <p class="text-text-subtle text-sm italic text-center py-2">Loading...</p>
        {/if}
    </div>
</section>

<!-- Configuration Hint -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <div class="border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-circle-info mr-1.5"></i> Configuration Reference
        </h2>
    </div>
    <div class="p-4 space-y-3 text-xs text-text-subtle">
        <p>Messaging and OAuth configuration is done via the TOML config file and environment variables. Changes require a container restart.</p>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div class="p-3 rounded bg-surface-elevated border border-border/50">
                <p class="font-medium text-text-muted mb-1.5">Telegram</p>
                <code class="block whitespace-pre-wrap text-text-subtle leading-relaxed">[telegram]
enabled = true
allowed_chat_ids = [123456]

# env: TELEGRAM_BOT_TOKEN</code>
            </div>
            <div class="p-3 rounded bg-surface-elevated border border-border/50">
                <p class="font-medium text-text-muted mb-1.5">WhatsApp</p>
                <code class="block whitespace-pre-wrap text-text-subtle leading-relaxed">[whatsapp]
enabled = true
bridge_port = 3033
webhook_port = 3030
allowed_numbers = ["+1234567890"]</code>
            </div>
            <div class="p-3 rounded bg-surface-elevated border border-border/50">
                <p class="font-medium text-text-muted mb-1.5">Dashboard SSO</p>
                <code class="block whitespace-pre-wrap text-text-subtle leading-relaxed">[dashboard]
password_enabled = true
sso_providers = ["google", "github"]
sso_allowed_emails = ["admin@example.com"]</code>
            </div>
            <div class="p-3 rounded bg-surface-elevated border border-border/50">
                <p class="font-medium text-text-muted mb-1.5">OAuth Providers</p>
                <code class="block whitespace-pre-wrap text-text-subtle leading-relaxed"># Per-provider env vars:
GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET
GITHUB_CLIENT_ID / GITHUB_CLIENT_SECRET
DISCORD_CLIENT_ID / DISCORD_CLIENT_SECRET
# ... etc. See config.example.toml</code>
            </div>
        </div>
    </div>
</section>

<!-- User Management -->
<section class="mt-6">
    <UsersPanel />
</section>
