<script lang="ts">
    import { onMount } from 'svelte';
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';

    let personality = $state('');
    let original = $state('');
    let saving = $state(false);
    let message = $state('');

    async function loadPersona() {
        try {
            const data = await api<{ personality: string }>('GET', '/api/persona');
            personality = data.personality ?? '';
            original = personality;
        } catch (e) {
            console.error('loadPersona:', e);
        }
    }

    async function savePersona() {
        saving = true;
        message = '';
        try {
            const resp = await api<{ ok: boolean }>('PUT', '/api/persona', { personality });
            if (resp.ok) {
                message = t('persona.saved');
                original = personality;
                setTimeout(() => message = '', 3000);
            } else {
                message = t('persona.failed');
            }
        } catch {
            message = t('persona.failed');
        }
        saving = false;
    }

    const dirty = $derived(personality !== original);

    onMount(loadPersona);
</script>

<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden mb-4">
    <div class="border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-masks-theater mr-1.5"></i> {t('persona.title')}
        </h2>
    </div>
    <div class="p-4 space-y-3">
        <p class="text-xs text-text-subtle">{t('persona.personality_hint')}</p>
        <div>
            <label class="text-xs font-medium text-text-muted uppercase tracking-wide mb-1 block">
                {t('persona.personality')}
            </label>
            <textarea
                bind:value={personality}
                rows="6"
                class="w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text text-sm
                       placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50 resize-y font-sans"
                placeholder="You are a helpful, proactive AI assistant..."
            ></textarea>
        </div>
        <div class="flex items-center gap-3">
            <button
                onclick={savePersona}
                disabled={saving || !dirty}
                class="px-4 py-2 rounded-md bg-primary-600 text-white font-medium text-sm
                       hover:bg-primary-500 transition-colors disabled:opacity-50"
            >
                {#if saving}
                    <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('persona.saving')}
                {:else}
                    <i class="fa-solid fa-floppy-disk mr-1"></i> {t('persona.save')}
                {/if}
            </button>
            {#if message}
                <span class="text-xs {message === t('persona.saved') ? 'text-success-500' : 'text-error-500'}">{message}</span>
            {/if}
            {#if dirty}
                <span class="text-xs text-warning-500">
                    <i class="fa-solid fa-circle-exclamation mr-0.5"></i> Unsaved changes
                </span>
            {/if}
        </div>
    </div>
</section>
