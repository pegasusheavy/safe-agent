<script lang="ts">
    import { onMount } from 'svelte';
    import { t } from '../lib/i18n';
    import { auth } from '../lib/state.svelte';

    interface PasskeyInfo {
        id: string;
        name: string;
        created_at: string;
    }

    interface TotpStatus {
        totp_enabled: boolean;
        passkey_count: number;
        passkeys_available: boolean;
    }

    let status = $state<TotpStatus | null>(null);
    let passkeys = $state<PasskeyInfo[]>([]);
    let loading = $state(true);
    let message = $state('');
    let messageType = $state<'success' | 'error'>('success');

    // TOTP setup flow
    let setupStep = $state<'idle' | 'showing_secret' | 'verifying'>('idle');
    let totpSecret = $state('');
    let otpauthUri = $state('');
    let recoveryCodes = $state<string[]>([]);
    let verifyCode = $state('');
    let showRecoveryCodes = $state(false);

    // Passkey registration
    let passkeyName = $state('');
    let registeringPasskey = $state(false);

    // Disable TOTP
    let disableCode = $state('');
    let disabling = $state(false);

    async function loadStatus() {
        loading = true;
        try {
            const [statusRes, passkeysRes] = await Promise.all([
                fetch('/api/auth/2fa/status'),
                fetch('/api/auth/passkeys'),
            ]);
            status = await statusRes.json();
            const pkData = await passkeysRes.json();
            passkeys = pkData.passkeys || [];
        } catch {
            setMessage(t('twofa.load_failed'), 'error');
        }
        loading = false;
    }

    function setMessage(msg: string, type: 'success' | 'error' = 'success') {
        message = msg;
        messageType = type;
        if (msg) setTimeout(() => { message = ''; }, 5000);
    }

    async function startTotpSetup() {
        try {
            const res = await fetch('/api/auth/2fa/setup', { method: 'POST' });
            const data = await res.json();
            if (data.ok) {
                totpSecret = data.secret;
                otpauthUri = data.otpauth_uri;
                recoveryCodes = data.recovery_codes;
                setupStep = 'showing_secret';
            } else {
                setMessage(data.error || 'Failed to set up TOTP', 'error');
            }
        } catch {
            setMessage('Failed to set up TOTP', 'error');
        }
    }

    async function enableTotp() {
        if (!verifyCode.trim()) return;
        try {
            const res = await fetch('/api/auth/2fa/enable', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ code: verifyCode.trim() }),
            });
            const data = await res.json();
            if (data.ok) {
                setMessage('TOTP 2FA enabled successfully');
                setupStep = 'idle';
                verifyCode = '';
                totpSecret = '';
                otpauthUri = '';
                await loadStatus();
            } else {
                setMessage(data.error || 'Invalid code', 'error');
            }
        } catch {
            setMessage('Failed to enable TOTP', 'error');
        }
    }

    async function disableTotp() {
        if (!disableCode.trim()) return;
        disabling = true;
        try {
            const res = await fetch('/api/auth/2fa/disable', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ code: disableCode.trim() }),
            });
            const data = await res.json();
            if (data.ok) {
                setMessage('TOTP 2FA disabled');
                disableCode = '';
                await loadStatus();
            } else {
                setMessage(data.error || 'Invalid code', 'error');
            }
        } catch {
            setMessage('Failed to disable TOTP', 'error');
        }
        disabling = false;
    }

    async function registerPasskey() {
        registeringPasskey = true;
        try {
            // Step 1: Start registration
            const startRes = await fetch('/api/auth/passkey/register/start', { method: 'POST' });
            const startData = await startRes.json();
            if (!startData.ok) {
                setMessage(startData.error || 'Failed to start passkey registration', 'error');
                registeringPasskey = false;
                return;
            }

            // Step 2: Create credential via browser WebAuthn API
            const options = startData.options;
            options.publicKey.challenge = base64urlToBuffer(options.publicKey.challenge);
            options.publicKey.user.id = base64urlToBuffer(options.publicKey.user.id);
            if (options.publicKey.excludeCredentials) {
                for (const cred of options.publicKey.excludeCredentials) {
                    cred.id = base64urlToBuffer(cred.id);
                }
            }

            const credential = await navigator.credentials.create(options) as PublicKeyCredential;
            if (!credential) {
                setMessage('Passkey creation was cancelled', 'error');
                registeringPasskey = false;
                return;
            }

            // Step 3: Finish registration
            const attestationResponse = credential.response as AuthenticatorAttestationResponse;
            const finishBody = {
                credential: {
                    id: credential.id,
                    rawId: bufferToBase64url(credential.rawId),
                    response: {
                        attestationObject: bufferToBase64url(attestationResponse.attestationObject),
                        clientDataJSON: bufferToBase64url(attestationResponse.clientDataJSON),
                    },
                    type: credential.type,
                },
                name: passkeyName || 'Passkey',
            };

            const finishRes = await fetch('/api/auth/passkey/register/finish', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(finishBody),
            });
            const finishData = await finishRes.json();
            if (finishData.ok) {
                setMessage('Passkey registered successfully');
                passkeyName = '';
                await loadStatus();
            } else {
                setMessage(finishData.error || 'Failed to register passkey', 'error');
            }
        } catch (e: any) {
            if (e.name === 'NotAllowedError') {
                setMessage('Passkey creation was cancelled or timed out', 'error');
            } else {
                setMessage(`Passkey registration failed: ${e.message}`, 'error');
            }
        }
        registeringPasskey = false;
    }

    async function deletePasskey(pk: PasskeyInfo) {
        if (!confirm(`Delete passkey "${pk.name}"?`)) return;
        try {
            const res = await fetch(`/api/auth/passkeys/${pk.id}`, { method: 'DELETE' });
            const data = await res.json();
            if (data.ok) {
                setMessage('Passkey deleted');
                await loadStatus();
            } else {
                setMessage(data.error || 'Failed to delete passkey', 'error');
            }
        } catch {
            setMessage('Failed to delete passkey', 'error');
        }
    }

    function base64urlToBuffer(base64url: string): ArrayBuffer {
        const base64 = base64url.replace(/-/g, '+').replace(/_/g, '/');
        const padded = base64 + '='.repeat((4 - base64.length % 4) % 4);
        const binary = atob(padded);
        const buffer = new ArrayBuffer(binary.length);
        const view = new Uint8Array(buffer);
        for (let i = 0; i < binary.length; i++) view[i] = binary.charCodeAt(i);
        return buffer;
    }

    function bufferToBase64url(buffer: ArrayBuffer): string {
        const bytes = new Uint8Array(buffer);
        let binary = '';
        for (const b of bytes) binary += String.fromCharCode(b);
        return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
    }

    function timeAgo(date: string): string {
        const d = new Date(date + 'Z');
        const diff = Date.now() - d.getTime();
        if (diff < 60000) return 'Just now';
        if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
        if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
        return `${Math.floor(diff / 86400000)}d ago`;
    }

    const hasUserId = $derived(!!auth.userId);

    onMount(() => {
        if (hasUserId) loadStatus();
    });
</script>

{#if !hasUserId}
    <div class="card">
        <p class="text-text-muted text-sm">
            <i class="fa-solid fa-circle-info mr-1"></i>
            {t('twofa.multi_user_only')}
        </p>
    </div>
{:else if loading}
    <div class="card">
        <p class="text-text-muted text-sm"><i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('twofa.loading_status')}</p>
    </div>
{:else if status}
    <div class="space-y-4">
        {#if message}
            <div class="alert {messageType === 'error' ? 'alert--error' : 'alert--success'}">
                <i class="fa-solid {messageType === 'error' ? 'fa-circle-exclamation' : 'fa-check-circle'} mr-1"></i>
                {message}
            </div>
        {/if}

        <!-- TOTP Section -->
        <div class="rounded-lg border border-border overflow-hidden">
            <div class="flex items-center justify-between p-3 bg-surface-elevated">
                <div class="flex items-center gap-2.5">
                    <i class="fa-solid fa-clock text-primary-400"></i>
                    <div>
                        <span class="text-sm font-medium text-text">{t('twofa.authenticator_app')}</span>
                        <p class="text-xs text-text-subtle mt-0.5">{t('twofa.authenticator_hint')}</p>
                    </div>
                </div>
                {#if status.totp_enabled}
                    <span class="badge badge--success">{t('twofa.enabled_badge')}</span>
                {:else}
                    <span class="badge">{t('twofa.disabled_badge')}</span>
                {/if}
            </div>
            <div class="p-4">
                {#if status.totp_enabled}
                    <!-- Disable TOTP -->
                    <p class="text-sm text-text-subtle mb-3">
                        {t('twofa.totp_enabled')}
                    </p>
                    <div class="flex gap-2 items-end">
                        <div>
                            <label class="form__label">{t('twofa.totp_code')}</label>
                            <input
                                type="text"
                                bind:value={disableCode}
                                maxlength="6"
                                placeholder="000000"
                                class="w-32 bg-bg border border-border rounded px-3 py-1.5 text-sm font-mono tracking-widest text-center"
                            />
                        </div>
                        <button
                            onclick={disableTotp}
                            disabled={disabling || disableCode.trim().length !== 6}
                            class="px-3 py-1.5 text-sm rounded bg-red-600/80 text-white hover:bg-red-500 transition-colors disabled:opacity-50"
                        >
                            {#if disabling}
                                <i class="fa-solid fa-spinner fa-spin mr-1"></i>
                            {/if}
                            {t('twofa.disable_2fa')}
                        </button>
                    </div>
                {:else if setupStep === 'idle'}
                    <!-- Start setup -->
                    <p class="text-sm text-text-subtle mb-3">
                        {t('twofa.setup_desc')}
                    </p>
                    <button
                        onclick={startTotpSetup}
                        class="btn btn--primary btn--md"
                    >
                        <i class="fa-solid fa-shield-halved mr-1"></i> {t('twofa.setup_authenticator')}
                    </button>
                {:else if setupStep === 'showing_secret'}
                    <!-- Show secret + verify -->
                    <div class="space-y-4">
                        <div class="p-4 rounded-lg bg-surface border border-border">
                            <p class="text-sm text-text mb-2 font-medium">{t('twofa.scan_step')}</p>
                            <div class="bg-white p-4 rounded-lg inline-block">
                                <!-- QR code rendered via otpauth URI — using a simple text fallback -->
                                <img
                                    src="https://api.qrserver.com/v1/create-qr-code/?size=200x200&data={encodeURIComponent(otpauthUri)}"
                                    alt="TOTP QR Code"
                                    class="w-48 h-48"
                                />
                            </div>
                            <p class="text-xs text-text-muted mt-3">{t('twofa.manual_key')}</p>
                            <code class="block mt-1 px-3 py-2 rounded bg-bg border border-border text-sm font-mono break-all select-all">
                                {totpSecret}
                            </code>
                        </div>

                        <div class="p-4 rounded-lg bg-surface border border-border">
                            <p class="text-sm text-text mb-2 font-medium">{t('twofa.save_recovery')}</p>
                            <p class="text-xs text-text-subtle mb-2">
                                {t('twofa.recovery_hint')}
                            </p>
                            <button
                                onclick={() => showRecoveryCodes = !showRecoveryCodes}
                                class="text-xs text-primary-400 hover:text-primary-300 mb-2"
                            >
                                {showRecoveryCodes ? t('twofa.hide_codes') : t('twofa.show_codes')}
                            </button>
                            {#if showRecoveryCodes}
                                <div class="grid grid-cols-2 gap-1 font-mono text-xs">
                                    {#each recoveryCodes as code, i}
                                        <div class="px-2 py-1 rounded bg-bg border border-border">
                                            <span class="text-text-muted mr-1">{i + 1}.</span> {code}
                                        </div>
                                    {/each}
                                </div>
                            {/if}
                        </div>

                        <div class="p-4 rounded-lg bg-surface border border-border">
                            <p class="text-sm text-text mb-2 font-medium">{t('twofa.verify_step')}</p>
                            <p class="text-xs text-text-subtle mb-2">{t('twofa.verify_hint')}</p>
                            <div class="flex gap-2">
                                <input
                                    type="text"
                                    bind:value={verifyCode}
                                    maxlength="6"
                                    placeholder="000000"
                                    class="w-32 bg-bg border border-border rounded px-3 py-1.5 text-sm font-mono tracking-widest text-center"
                                    onkeydown={(e) => { if (e.key === 'Enter') enableTotp(); }}
                                />
                                <button
                                    onclick={enableTotp}
                                    disabled={verifyCode.trim().length !== 6}
                                    class="px-4 py-1.5 text-sm rounded bg-green-600 text-white hover:bg-green-500 transition-colors disabled:opacity-50"
                                >
                                    <i class="fa-solid fa-check mr-1"></i> {t('twofa.verify_enable')}
                                </button>
                                <button
                                    onclick={() => { setupStep = 'idle'; totpSecret = ''; otpauthUri = ''; }}
                                    class="btn btn--secondary btn--sm"
                                >
                                    {t('common.cancel')}
                                </button>
                            </div>
                        </div>
                    </div>
                {/if}
            </div>
        </div>

        <!-- Passkeys Section -->
        {#if status.passkeys_available}
            <div class="rounded-lg border border-border overflow-hidden">
                <div class="flex items-center justify-between p-3 bg-surface-elevated">
                    <div class="flex items-center gap-2.5">
                        <i class="fa-solid fa-fingerprint text-amber-400"></i>
                        <div>
                        <span class="text-sm font-medium text-text">{t('twofa.passkeys')}</span>
                        <p class="text-xs text-text-subtle mt-0.5">{t('twofa.passkeys_desc')}</p>
                        </div>
                    </div>
                    {#if status.passkey_count > 0}
                        <span class="badge badge--success">
                            {t('twofa.registered', { count: status.passkey_count })}
                        </span>
                    {:else}
                        <span class="badge">{t('twofa.none_badge')}</span>
                    {/if}
                </div>
                <div class="p-4">
                    <!-- Registered passkeys list -->
                    {#if passkeys.length > 0}
                        <div class="space-y-2 mb-4">
                            {#each passkeys as pk}
                                <div class="flex items-center justify-between p-2.5 rounded-md bg-surface border border-border/50">
                                    <div class="flex items-center gap-2.5">
                                        <i class="fa-solid fa-key text-text-muted"></i>
                                        <div>
                                            <span class="text-sm text-text">{pk.name}</span>
                                            <span class="text-xs text-text-subtle ml-2">{t('twofa.added_label')} {timeAgo(pk.created_at)}</span>
                                        </div>
                                    </div>
                                    <button
                                        onclick={() => deletePasskey(pk)}
                                        class="text-xs px-2 py-1 rounded bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors"
                                        title={t('twofa.delete_passkey')}
                                    >
                                        <i class="fa-solid fa-trash"></i>
                                    </button>
                                </div>
                            {/each}
                        </div>
                    {/if}

                    <!-- Register new passkey -->
                    <div class="flex items-end gap-2">
                        <div class="flex-1">
                            <label class="form__label">{t('twofa.passkey_name')}</label>
                            <input
                                type="text"
                                bind:value={passkeyName}
                                placeholder={t('twofa.passkey_placeholder')}
                                class="form__input"
                            />
                        </div>
                        <button
                            onclick={registerPasskey}
                            disabled={registeringPasskey}
                            class="btn btn--primary btn--md disabled:opacity-50 whitespace-nowrap"
                        >
                            {#if registeringPasskey}
                                <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('twofa.registering')}
                            {:else}
                                <i class="fa-solid fa-plus mr-1"></i> {t('twofa.add_passkey')}
                            {/if}
                        </button>
                    </div>

                    {#if passkeys.length === 0}
                        <p class="text-xs text-text-subtle mt-3">
                            <i class="fa-solid fa-circle-info mr-1"></i>
                            {t('twofa.passkey_info')}
                        </p>
                    {/if}
                </div>
            </div>
        {/if}

        <!-- Info box -->
        <div class="p-3 rounded-lg bg-surface-elevated border border-border/50">
            <p class="text-xs text-text-subtle">
                <i class="fa-solid fa-circle-info mr-1"></i>
                {t('twofa.info_box')}
                {#if status.totp_enabled && status.passkey_count > 0}
                    {t('twofa.info_both')}
                {/if}
            </p>
        </div>
    </div>
{/if}
