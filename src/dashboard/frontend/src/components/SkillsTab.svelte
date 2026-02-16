<script lang="ts">
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import type { SkillStatus } from '../lib/types';
    import SkillCard from './SkillCard.svelte';

    let skills = $state<SkillStatus[]>([]);
    let error = $state(false);

    async function load() {
        error = false;
        try {
            skills = await api<SkillStatus[]>('GET', '/api/skills');
        } catch (e) {
            error = true;
            console.error('loadSkills:', e);
        }
    }

    $effect(() => {
        if (dashboard.currentTab === 'skills') {
            dashboard.refreshCounter;
            load();
        }
    });
</script>

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
                <SkillCard {skill} onrefresh={load} />
            {/each}
        {/if}
    </div>
</section>
