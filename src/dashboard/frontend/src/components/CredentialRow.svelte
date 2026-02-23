<script lang="ts">
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import type { CredentialStatus, ActionResponse } from '../lib/types';

    interface Props {
        credential: CredentialStatus;
        skillName: string;
        onchange: () => void;
    }

    let { credential, skillName, onchange }: Props = $props();
    let inputValue = $state('');

    function dotColor(): string {
        if (credential.configured) return 'bg-success-500';
        return credential.required ? 'bg-error-500' : 'bg-warning-500';
    }

    function statusLabel(): string {
        if (credential.configured) return t('cred.configured');
        return credential.required ? t('cred.required') : t('cred.optional');
    }

    function placeholder(): string {
        if (credential.configured) return t('cred.configured_placeholder');
        return credential.description || t('cred.enter_value');
    }

    async function save() {
        const value = inputValue.trim();
        if (!value) return;

        try {
            await api<ActionResponse>(
                'PUT',
                `/api/skills/${encodeURIComponent(skillName)}/credentials`,
                { key: credential.name, value },
            );
            inputValue = '';
            onchange();
        } catch (e) {
            console.error('saveCredential:', e);
            alert(t('cred.save_failed') + (e as Error).message);
        }
    }

    async function remove() {
        if (!confirm(t('cred.remove_confirm', { name: credential.name, skill: skillName }))) return;
        try {
            await api<ActionResponse>(
                'DELETE',
                `/api/skills/${encodeURIComponent(skillName)}/credentials/${encodeURIComponent(credential.name)}`,
            );
            onchange();
        } catch (e) {
            console.error('deleteCredential:', e);
            alert(t('cred.remove_failed') + (e as Error).message);
        }
    }

    function handleKey(e: KeyboardEvent) {
        if (e.key === 'Enter') save();
    }
</script>

<div class="flex items-center gap-2 mb-2 text-sm">
    <div class="w-2 h-2 rounded-full shrink-0 {dotColor()}" title={statusLabel()}></div>
    <div
        class="font-mono text-xs min-w-[200px] shrink-0 text-accent-300"
        title={credential.description}
    >
        {credential.label || credential.name}
    </div>
    <input
        type="password"
        bind:value={inputValue}
        onkeyup={handleKey}
        placeholder={placeholder()}
        class="flex-1 px-2 py-1 border border-border rounded-md bg-background text-text text-xs font-mono outline-none focus:border-primary-500 focus:ring-1 focus:ring-primary-900 placeholder:text-text-subtle"
    />
    <div class="flex gap-1 shrink-0">
        <button
            onclick={save}
            title={t('common.save')}
            class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface text-success-500 hover:bg-success-500/10 hover:border-success-500 transition-colors"
        >
            <i class="fa-solid fa-floppy-disk"></i>
        </button>
        {#if credential.configured}
            <button
                onclick={remove}
                title={t('common.delete')}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface text-error-500 hover:bg-error-500/10 hover:border-error-500 transition-colors"
            >
                <i class="fa-solid fa-trash-can"></i>
            </button>
        {/if}
    </div>
</div>
