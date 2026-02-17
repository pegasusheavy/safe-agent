<script lang="ts">
    import { auth } from '../lib/state.svelte';

    interface SsoProvider {
        id: string;
        name: string;
        icon: string;
        login_url: string;
    }

    let username = $state('');
    let password = $state('');
    let error = $state('');
    let loading = $state(false);
    let passwordEnabled = $state(true);
    let ssoProviders = $state<SsoProvider[]>([]);
    let infoLoaded = $state(false);
    let multiUserMode = $state(false);

    // 2FA challenge state
    let twoFaRequired = $state(false);
    let challengeToken = $state('');
    let twoFaMethods = $state<string[]>([]);
    let twoFaUserId = $state('');
    let totpCode = $state('');
    let recoveryCode = $state('');
    let showRecoveryInput = $state(false);
    let passkeyLoading = $state(false);

    $effect(() => {
        loadLoginInfo();
    });

    async function loadLoginInfo() {
        try {
            const res = await fetch('/api/auth/info');
            const data = await res.json();
            passwordEnabled = data.password_enabled ?? true;
            ssoProviders = data.sso_providers ?? [];
            multiUserMode = data.multi_user ?? false;
        } catch {
            passwordEnabled = true;
            ssoProviders = [];
        } finally {
            infoLoaded = true;
        }
    }

    async function handleLogin(e: SubmitEvent) {
        e.preventDefault();
        error = '';
        loading = true;

        try {
            const loginBody: any = { password };
            if (multiUserMode && username) {
                loginBody.username = username;
            }

            const res = await fetch('/api/auth/login', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(loginBody),
            });

            const data = await res.json();

            if (data.requires_2fa) {
                // Enter 2FA challenge mode
                twoFaRequired = true;
                challengeToken = data.challenge_token;
                twoFaMethods = data.methods || [];
                twoFaUserId = data.user_id || '';
                password = '';
            } else if (res.ok && data.ok) {
                completeLogin(data);
            } else {
                error = data.error || 'Login failed';
            }
        } catch {
            error = 'Connection failed';
        } finally {
            loading = false;
        }
    }

    async function verifyTotp() {
        error = '';
        loading = true;

        try {
            const body: any = { challenge_token: challengeToken };
            if (showRecoveryInput) {
                body.recovery_code = recoveryCode.trim();
            } else {
                body.totp_code = totpCode.trim();
            }

            const res = await fetch('/api/auth/2fa/verify', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });

            const data = await res.json();
            if (res.ok && data.ok) {
                completeLogin(data);
            } else {
                error = data.error || 'Verification failed';
            }
        } catch {
            error = 'Connection failed';
        } finally {
            loading = false;
        }
    }

    async function startPasskeyAuth() {
        error = '';
        passkeyLoading = true;

        try {
            // Step 1: Get challenge options
            const startRes = await fetch('/api/auth/passkey/authenticate/start', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ challenge_token: challengeToken }),
            });
            const startData = await startRes.json();
            if (!startData.ok) {
                error = startData.error || 'Failed to start passkey authentication';
                passkeyLoading = false;
                return;
            }

            // Step 2: Prompt user for passkey via browser WebAuthn API
            const options = startData.options;
            options.publicKey.challenge = base64urlToBuffer(options.publicKey.challenge);
            if (options.publicKey.allowCredentials) {
                for (const cred of options.publicKey.allowCredentials) {
                    cred.id = base64urlToBuffer(cred.id);
                }
            }

            const credential = await navigator.credentials.get(options) as PublicKeyCredential;
            if (!credential) {
                error = 'Passkey authentication was cancelled';
                passkeyLoading = false;
                return;
            }

            // Step 3: Send assertion to server
            const assertionResponse = credential.response as AuthenticatorAssertionResponse;
            const finishBody = {
                challenge_token: challengeToken,
                credential: {
                    id: credential.id,
                    rawId: bufferToBase64url(credential.rawId),
                    response: {
                        authenticatorData: bufferToBase64url(assertionResponse.authenticatorData),
                        clientDataJSON: bufferToBase64url(assertionResponse.clientDataJSON),
                        signature: bufferToBase64url(assertionResponse.signature),
                        userHandle: assertionResponse.userHandle
                            ? bufferToBase64url(assertionResponse.userHandle)
                            : null,
                    },
                    type: credential.type,
                },
            };

            const finishRes = await fetch('/api/auth/passkey/authenticate/finish', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(finishBody),
            });

            const finishData = await finishRes.json();
            if (finishRes.ok && finishData.ok) {
                completeLogin(finishData);
            } else {
                error = finishData.error || 'Passkey verification failed';
            }
        } catch (e: any) {
            if (e.name === 'NotAllowedError') {
                error = 'Passkey authentication was cancelled or timed out';
            } else {
                error = `Passkey authentication failed: ${e.message}`;
            }
        } finally {
            passkeyLoading = false;
        }
    }

    function completeLogin(data: any) {
        auth.authenticated = true;
        if (data.user) {
            auth.userId = data.user.id;
            auth.role = data.user.role;
            auth.subject = data.user.username;
        }
        // Reset all state
        username = '';
        password = '';
        twoFaRequired = false;
        challengeToken = '';
        totpCode = '';
        recoveryCode = '';
    }

    function backToLogin() {
        twoFaRequired = false;
        challengeToken = '';
        twoFaMethods = [];
        totpCode = '';
        recoveryCode = '';
        showRecoveryInput = false;
        error = '';
    }

    function startSso(provider: SsoProvider) {
        window.location.href = provider.login_url;
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
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
    <div class="w-full max-w-sm mx-4 p-8 rounded-xl border border-border bg-surface shadow-2xl">
        <div class="text-center mb-6">
            <i class="fa-solid fa-robot text-3xl text-primary-400 mb-3"></i>
            <h2 class="text-lg font-semibold text-text">safe-agent</h2>
            <p class="text-sm text-text-muted mt-1">
                {#if twoFaRequired}
                    Two-factor authentication required
                {:else if passwordEnabled && ssoProviders.length > 0}
                    Sign in to continue
                {:else if ssoProviders.length > 0}
                    Sign in with your account
                {:else}
                    Enter the dashboard password to continue
                {/if}
            </p>
        </div>

        {#if error}
            <div class="mb-4 px-3 py-2 rounded-md bg-error-950 border border-error-500/30 text-sm text-error-400">
                <i class="fa-solid fa-circle-exclamation mr-1"></i> {error}
            </div>
        {/if}

        {#if !infoLoaded}
            <div class="text-center py-4">
                <i class="fa-solid fa-spinner fa-spin text-text-muted"></i>
            </div>
        {:else if twoFaRequired}
            <!-- 2FA Challenge Flow -->
            <div class="space-y-4">
                {#if twoFaMethods.includes('passkey')}
                    <button
                        type="button"
                        onclick={startPasskeyAuth}
                        disabled={passkeyLoading}
                        class="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-md
                               border border-border bg-surface-elevated text-text font-medium text-sm
                               hover:bg-surface hover:border-primary-500/40 transition-colors
                               disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                        {#if passkeyLoading}
                            <i class="fa-solid fa-spinner fa-spin"></i> Waiting for passkey...
                        {:else}
                            <i class="fa-solid fa-fingerprint text-amber-400"></i> Use Passkey
                        {/if}
                    </button>
                {/if}

                {#if twoFaMethods.includes('passkey') && twoFaMethods.includes('totp')}
                    <div class="relative my-2">
                        <div class="absolute inset-0 flex items-center">
                            <div class="w-full border-t border-border"></div>
                        </div>
                        <div class="relative flex justify-center text-xs uppercase">
                            <span class="bg-surface px-3 text-text-muted tracking-wider">or</span>
                        </div>
                    </div>
                {/if}

                {#if twoFaMethods.includes('totp')}
                    {#if !showRecoveryInput}
                        <div>
                            <label class="block mb-3">
                                <span class="text-xs font-medium text-text-muted uppercase tracking-wide">Authenticator Code</span>
                                <input
                                    type="text"
                                    bind:value={totpCode}
                                    maxlength="6"
                                    autofocus
                                    disabled={loading}
                                    class="mt-1 w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text
                                           font-mono tracking-widest text-center text-lg
                                           placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50
                                           disabled:opacity-50"
                                    placeholder="000000"
                                    onkeydown={(e) => { if (e.key === 'Enter' && totpCode.trim().length === 6) verifyTotp(); }}
                                />
                            </label>
                            <button
                                type="button"
                                onclick={verifyTotp}
                                disabled={loading || totpCode.trim().length !== 6}
                                class="w-full px-4 py-2 rounded-md bg-primary-600 text-white font-medium text-sm
                                       hover:bg-primary-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                {#if loading}
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i> Verifying...
                                {:else}
                                    <i class="fa-solid fa-check mr-1"></i> Verify
                                {/if}
                            </button>
                        </div>
                        <button
                            type="button"
                            onclick={() => { showRecoveryInput = true; error = ''; }}
                            class="w-full text-xs text-text-muted hover:text-primary-400 transition-colors"
                        >
                            Use a recovery code instead
                        </button>
                    {:else}
                        <div>
                            <label class="block mb-3">
                                <span class="text-xs font-medium text-text-muted uppercase tracking-wide">Recovery Code</span>
                                <input
                                    type="text"
                                    bind:value={recoveryCode}
                                    autofocus
                                    disabled={loading}
                                    class="mt-1 w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text
                                           font-mono tracking-wider text-center
                                           placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50
                                           disabled:opacity-50"
                                    placeholder="abcd1234"
                                    onkeydown={(e) => { if (e.key === 'Enter' && recoveryCode.trim()) verifyTotp(); }}
                                />
                            </label>
                            <button
                                type="button"
                                onclick={verifyTotp}
                                disabled={loading || !recoveryCode.trim()}
                                class="w-full px-4 py-2 rounded-md bg-primary-600 text-white font-medium text-sm
                                       hover:bg-primary-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                {#if loading}
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i> Verifying...
                                {:else}
                                    <i class="fa-solid fa-check mr-1"></i> Verify Recovery Code
                                {/if}
                            </button>
                        </div>
                        <button
                            type="button"
                            onclick={() => { showRecoveryInput = false; error = ''; }}
                            class="w-full text-xs text-text-muted hover:text-primary-400 transition-colors"
                        >
                            Use authenticator code instead
                        </button>
                    {/if}
                {/if}

                <button
                    type="button"
                    onclick={backToLogin}
                    class="w-full text-xs text-text-muted hover:text-text transition-colors mt-2"
                >
                    <i class="fa-solid fa-arrow-left mr-1"></i> Back to login
                </button>
            </div>
        {:else}
            <!-- Normal Login Flow -->
            {#if ssoProviders.length > 0}
                <div class="space-y-2 mb-4">
                    {#each ssoProviders as provider}
                        <button
                            type="button"
                            onclick={() => startSso(provider)}
                            class="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-md
                                   border border-border bg-surface-elevated text-text font-medium text-sm
                                   hover:bg-surface hover:border-primary-500/40 transition-colors"
                        >
                            <i class="{provider.icon} text-base"></i>
                            Continue with {provider.name}
                        </button>
                    {/each}
                </div>

                {#if passwordEnabled}
                    <div class="relative my-5">
                        <div class="absolute inset-0 flex items-center">
                            <div class="w-full border-t border-border"></div>
                        </div>
                        <div class="relative flex justify-center text-xs uppercase">
                            <span class="bg-surface px-3 text-text-muted tracking-wider">or</span>
                        </div>
                    </div>
                {/if}
            {/if}

            {#if passwordEnabled}
                <form onsubmit={handleLogin}>
                    {#if multiUserMode}
                        <label class="block mb-3">
                            <span class="text-xs font-medium text-text-muted uppercase tracking-wide">Username</span>
                            <input
                                type="text"
                                bind:value={username}
                                required
                                autofocus
                                disabled={loading}
                                class="mt-1 w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text
                                       placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50
                                       disabled:opacity-50"
                                placeholder="Enter username"
                            />
                        </label>
                    {/if}
                    <label class="block mb-4">
                        <span class="text-xs font-medium text-text-muted uppercase tracking-wide">Password</span>
                        <input
                            type="password"
                            bind:value={password}
                            required
                            autofocus={!multiUserMode}
                            disabled={loading}
                            class="mt-1 w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text
                                   placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50
                                   disabled:opacity-50"
                            placeholder="Enter password"
                        />
                    </label>

                    <button
                        type="submit"
                        disabled={loading || !password}
                        class="w-full px-4 py-2 rounded-md bg-primary-600 text-white font-medium text-sm
                               hover:bg-primary-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                        {#if loading}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> Signing in...
                        {:else}
                            <i class="fa-solid fa-right-to-bracket mr-1"></i> Sign In
                        {/if}
                    </button>
                </form>
            {/if}
        {/if}
    </div>
</div>
