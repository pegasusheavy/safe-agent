<script lang="ts">
    import { api } from '../lib/api';
    import type { SkillStatus, ActionResponse } from '../lib/types';
    import CredentialRow from './CredentialRow.svelte';

    interface Props {
        skill: SkillStatus;
        onrefresh: () => void;
    }

    let { skill, onrefresh }: Props = $props();

    async function restart() {
        try {
            await api<ActionResponse>(
                'POST',
                `/api/skills/${encodeURIComponent(skill.name)}/restart`,
            );
            setTimeout(onrefresh, 1000);
        } catch (e) {
            console.error('restartSkill:', e);
            alert('Failed to restart skill: ' + (e as Error).message);
        }
    }
</script>

<div class="p-4 border border-border rounded-md mb-3 bg-surface-muted">
    <div class="flex justify-between items-center mb-2">
        <span class="text-[15px] font-semibold">{skill.name}</span>
        <div class="flex items-center gap-2">
            {#if skill.skill_type === 'daemon'}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-primary-950 text-primary-400">
                    <i class="fa-solid fa-server mr-1"></i>daemon
                </span>
            {:else}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-warning-500/15 text-warning-500">
                    <i class="fa-solid fa-bolt mr-1"></i>oneshot
                </span>
            {/if}

            {#if skill.running}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-success-500/15 text-success-500">
                    <i class="fa-solid fa-circle-check mr-1"></i>running
                </span>
            {:else}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-error-500/12 text-error-400">
                    <i class="fa-solid fa-circle-stop mr-1"></i>stopped
                </span>
            {/if}

            {#if skill.pid}
                <span class="text-[11px] text-text-subtle font-mono">PID {skill.pid}</span>
            {/if}

            <button
                onclick={restart}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-rotate-right mr-1"></i>Restart
            </button>
        </div>
    </div>

    {#if skill.description}
        <div class="text-sm text-text-muted mb-3">{skill.description}</div>
    {/if}

    {#if skill.credentials?.length}
        <div class="border-t border-border pt-3">
            <div class="text-xs font-semibold uppercase tracking-wider text-accent-500 mb-2">
                <i class="fa-solid fa-key mr-1"></i> Credentials
            </div>
            {#each skill.credentials as cred (cred.name)}
                <CredentialRow credential={cred} skillName={skill.name} onchange={onrefresh} />
            {/each}
        </div>
    {/if}
</div>
