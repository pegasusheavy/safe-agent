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

<section class="card mb-4">
    <div class="card__header">
        <h2 class="card__header-title">
            <i class="fa-solid fa-masks-theater mr-1.5"></i> {t('persona.title')}
        </h2>
    </div>
    <div class="card__body space-y-3">
        <p class="text-xs text-text-subtle">{t('persona.personality_hint')}</p>
        <div>
            <label class="form__label">
                {t('persona.personality')}
            </label>
            <textarea
                bind:value={personality}
                rows="6"
                class="form__textarea resize-y font-sans"
                placeholder="You are a helpful, proactive AI assistant..."
            ></textarea>
        </div>
        <div class="flex items-center gap-3">
            <button
                onclick={savePersona}
                disabled={saving || !dirty}
                class="btn btn--primary btn--md disabled:opacity-50"
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
