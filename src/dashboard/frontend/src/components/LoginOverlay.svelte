<script lang="ts">
    import { auth } from '../lib/state.svelte';

    let password = $state('');
    let error = $state('');
    let loading = $state(false);

    async function handleLogin(e: SubmitEvent) {
        e.preventDefault();
        error = '';
        loading = true;

        try {
            const res = await fetch('/api/auth/login', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ password }),
            });

            const data = await res.json();

            if (res.ok && data.ok) {
                auth.authenticated = true;
                password = '';
            } else {
                error = data.error || 'Login failed';
            }
        } catch {
            error = 'Connection failed';
        } finally {
            loading = false;
        }
    }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
    <form
        onsubmit={handleLogin}
        class="w-full max-w-sm mx-4 p-8 rounded-xl border border-border bg-surface shadow-2xl"
    >
        <div class="text-center mb-6">
            <i class="fa-solid fa-robot text-3xl text-primary-400 mb-3"></i>
            <h2 class="text-lg font-semibold text-text">safe-agent</h2>
            <p class="text-sm text-text-muted mt-1">Enter the dashboard password to continue</p>
        </div>

        {#if error}
            <div class="mb-4 px-3 py-2 rounded-md bg-error-950 border border-error-500/30 text-sm text-error-400">
                <i class="fa-solid fa-circle-exclamation mr-1"></i> {error}
            </div>
        {/if}

        <label class="block mb-4">
            <span class="text-xs font-medium text-text-muted uppercase tracking-wide">Password</span>
            <input
                type="password"
                bind:value={password}
                required
                autofocus
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
</div>
