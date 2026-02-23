<script lang="ts">
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import type { SkillStatus, ActionResponse } from '../lib/types';
    import SkillCard from './SkillCard.svelte';
    import SkillMarketplace from './SkillMarketplace.svelte';

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
    let extensions = $state<SkillExtInfo[]>([]);

    // Import form state
    let showImport = $state(false);
    let importSource = $state<'git' | 'url' | 'path'>('git');
    let importLocation = $state('');
    let importName = $state('');
    let importing = $state(false);
    let importError = $state('');
    let importSuccess = $state('');

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

    async function importSkill() {
        const loc = importLocation.trim();
        if (!loc) return;
        importing = true;
        importError = '';
        importSuccess = '';
        try {
            const body: Record<string, unknown> = { source: importSource, location: loc };
            const name = importName.trim();
            if (name) body.name = name;
            const res = await api<ActionResponse>('POST', '/api/skills/import', body);
            if (res.ok) {
                importSuccess = res.message ?? 'Skill imported successfully';
                importLocation = '';
                importName = '';
                setTimeout(() => { importSuccess = ''; showImport = false; }, 3000);
                load();
                loadExtensions();
            } else {
                importError = res.message ?? 'Import failed';
            }
        } catch (e) {
            importError = (e as Error).message;
        } finally {
            importing = false;
        }
    }

    $effect(() => {
        if (dashboard.currentTab === 'skills') {
            dashboard.refreshCounter;
            load();
            loadExtensions();
        }
    });
</script>

<!-- Skills List -->
<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-puzzle-piece mr-1.5"></i> {t('skills.title')}
        </h2>
        <div class="flex items-center gap-2 pr-3">
            <span class="text-xs text-text-muted">
                {t('skills.count', { count: skills.length })}
            </span>
            <button
                onclick={() => { showImport = !showImport; importError = ''; importSuccess = ''; }}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-primary-500/10 hover:border-primary-500 text-primary-400 transition-colors"
            >
                <i class="fa-solid fa-file-import mr-1"></i> Import
            </button>
            <button
                onclick={load}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-arrows-rotate mr-1"></i> {t('common.refresh')}
            </button>
        </div>
    </div>

    <!-- Import panel -->
    {#if showImport}
        <div class="border-b border-border bg-surface-muted p-4">
            <div class="text-xs font-semibold uppercase tracking-wider text-primary-400 mb-3">
                <i class="fa-solid fa-file-import mr-1"></i> {t('skills.import_skill')}
            </div>

            <!-- Source type selector -->
            <div class="flex items-center gap-1 mb-3">
                <button
                    onclick={() => importSource = 'git'}
                    class="px-3 py-1.5 text-xs rounded-md transition-colors {importSource === 'git' ? 'bg-primary-500/20 text-primary-400 border border-primary-500/50' : 'border border-border text-text-muted hover:bg-surface-elevated'}"
                >
                    <i class="fa-brands fa-git-alt mr-1"></i> {t('skills.git_repo')}
                </button>
                <button
                    onclick={() => importSource = 'url'}
                    class="px-3 py-1.5 text-xs rounded-md transition-colors {importSource === 'url' ? 'bg-primary-500/20 text-primary-400 border border-primary-500/50' : 'border border-border text-text-muted hover:bg-surface-elevated'}"
                >
                    <i class="fa-solid fa-globe mr-1"></i> {t('skills.archive_url')}
                </button>
                <button
                    onclick={() => importSource = 'path'}
                    class="px-3 py-1.5 text-xs rounded-md transition-colors {importSource === 'path' ? 'bg-primary-500/20 text-primary-400 border border-primary-500/50' : 'border border-border text-text-muted hover:bg-surface-elevated'}"
                >
                    <i class="fa-solid fa-folder mr-1"></i> {t('skills.local_path')}
                </button>
            </div>

            <!-- Location input -->
            <div class="flex flex-col gap-2">
                <input
                    type="text"
                    bind:value={importLocation}
                    placeholder={importSource === 'git' ? t('skills.git_placeholder') : importSource === 'url' ? t('skills.url_placeholder') : t('skills.path_placeholder')}
                    class="w-full px-3 py-2 text-sm border border-border rounded-md bg-background text-text font-mono outline-none focus:border-primary-500 focus:ring-1 focus:ring-primary-900 placeholder:text-text-subtle"
                />
                <div class="flex items-center gap-2">
                    <input
                        type="text"
                        bind:value={importName}
                        placeholder={t('skills.name_placeholder')}
                        class="flex-1 px-3 py-2 text-sm border border-border rounded-md bg-background text-text outline-none focus:border-primary-500 focus:ring-1 focus:ring-primary-900 placeholder:text-text-subtle"
                    />
                    <button
                        onclick={importSkill}
                        disabled={importing || !importLocation.trim()}
                        class="px-4 py-2 text-xs font-semibold border border-primary-500/50 rounded-md bg-primary-500/15 text-primary-400 hover:bg-primary-500/25 transition-colors disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
                    >
                        {#if importing}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('skills.importing')}
                        {:else}
                            <i class="fa-solid fa-download mr-1"></i> {t('skills.import_skill')}
                        {/if}
                    </button>
                </div>
            </div>

            {#if importError}
                <div class="mt-2 text-xs text-error-400 p-2 bg-error-500/10 border border-error-500/30 rounded">
                    <i class="fa-solid fa-triangle-exclamation mr-1"></i>{importError}
                </div>
            {/if}
            {#if importSuccess}
                <div class="mt-2 text-xs text-success-500 p-2 bg-success-500/10 border border-success-500/30 rounded">
                    <i class="fa-solid fa-check mr-1"></i>{importSuccess}
                </div>
            {/if}

            <p class="mt-2 text-[11px] text-text-subtle">
                {t('skills.import_desc')}
            </p>
        </div>
    {/if}

    <div class="p-3">
        {#if error}
            <p class="text-text-subtle text-sm italic text-center py-4">{t('skills.error_loading')}</p>
        {:else if skills.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">
                {t('skills.no_skills_import')}
            </p>
        {:else}
            {#each skills as skill (skill.name)}
                <SkillCard {skill} onrefresh={load} extension={getExtension(skill.name)} />
            {/each}
        {/if}
    </div>
</section>

<SkillMarketplace />
