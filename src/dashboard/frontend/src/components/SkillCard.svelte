<script lang="ts">
    import { api } from '../lib/api';
    import type { SkillStatus, SkillDetail, ActionResponse } from '../lib/types';
    import CredentialRow from './CredentialRow.svelte';

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

    interface Props {
        skill: SkillStatus;
        onrefresh: () => void;
        extension?: SkillExtInfo | null;
    }

    let { skill, onrefresh, extension = null }: Props = $props();

    let expanded = $state(false);
    let detail = $state<SkillDetail | null>(null);
    let detailLoading = $state(false);
    let detailError = $state('');

    type TabName = 'params' | 'logs' | 'manifest' | 'plugin';
    let activeTab = $state<TabName>('params');

    let logContent = $state('');
    let logLoading = $state(false);

    let manifestEdit = $state('');
    let manifestSaving = $state(false);
    let manifestError = $state('');
    let manifestSuccess = $state('');

    let newEnvKey = $state('');
    let newEnvValue = $state('');
    let deleting = $state(false);

    async function toggleExpand() {
        expanded = !expanded;
        if (expanded && !detail) {
            await loadDetail();
        }
    }

    async function loadDetail() {
        detailLoading = true;
        detailError = '';
        try {
            detail = await api<SkillDetail>('GET', `/api/skills/${encodeURIComponent(skill.name)}/detail`);
            manifestEdit = detail.manifest_raw;
        } catch (e) {
            detailError = (e as Error).message;
        } finally {
            detailLoading = false;
        }
    }

    async function loadLog() {
        logLoading = true;
        try {
            const res = await api<{ log: string }>('GET', `/api/skills/${encodeURIComponent(skill.name)}/log?lines=300`);
            logContent = res.log;
        } catch (e) {
            logContent = `Error loading log: ${(e as Error).message}`;
        } finally {
            logLoading = false;
        }
    }

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

    async function toggleEnabled() {
        const newVal = !skill.enabled;
        try {
            await api<ActionResponse>(
                'PUT',
                `/api/skills/${encodeURIComponent(skill.name)}/enabled`,
                { enabled: newVal },
            );
            setTimeout(onrefresh, 500);
        } catch (e) {
            console.error('toggleEnabled:', e);
            alert('Failed to toggle skill: ' + (e as Error).message);
        }
    }

    async function saveManifest() {
        manifestSaving = true;
        manifestError = '';
        manifestSuccess = '';
        try {
            await api<ActionResponse>(
                'PUT',
                `/api/skills/${encodeURIComponent(skill.name)}/manifest`,
                { toml: manifestEdit },
            );
            manifestSuccess = 'Saved. Restart the skill for changes to take effect.';
            setTimeout(() => { manifestSuccess = ''; }, 5000);
            await loadDetail();
        } catch (e) {
            manifestError = (e as Error).message;
        } finally {
            manifestSaving = false;
        }
    }

    async function addEnvVar() {
        const k = newEnvKey.trim();
        const v = newEnvValue.trim();
        if (!k) return;
        try {
            await api<ActionResponse>(
                'PUT',
                `/api/skills/${encodeURIComponent(skill.name)}/env`,
                { key: k, value: v },
            );
            newEnvKey = '';
            newEnvValue = '';
            await loadDetail();
        } catch (e) {
            alert('Failed to set env var: ' + (e as Error).message);
        }
    }

    async function deleteEnvVar(key: string) {
        if (!confirm(`Remove env var "${key}" from "${skill.name}"?`)) return;
        try {
            await api<ActionResponse>(
                'DELETE',
                `/api/skills/${encodeURIComponent(skill.name)}/env/${encodeURIComponent(key)}`,
            );
            await loadDetail();
        } catch (e) {
            alert('Failed to delete env var: ' + (e as Error).message);
        }
    }

    async function deleteSkill() {
        if (!confirm(`Permanently delete skill "${skill.name}"? This cannot be undone.`)) return;
        deleting = true;
        try {
            await api<ActionResponse>(
                'DELETE',
                `/api/skills/${encodeURIComponent(skill.name)}`,
            );
            onrefresh();
        } catch (e) {
            console.error('deleteSkill:', e);
            alert('Failed to delete skill: ' + (e as Error).message);
        } finally {
            deleting = false;
        }
    }

    function switchTab(tab: TabName) {
        activeTab = tab;
        if (tab === 'logs') loadLog();
    }

    let hasExtension = $derived(
        extension !== null && (
            (extension.ui.panel != null) ||
            (extension.ui.page != null) ||
            extension.route_count > 0
        )
    );
</script>

<div class="border border-border rounded-md mb-3 bg-surface-muted overflow-hidden">
    <!-- Header row -->
    <button
        onclick={toggleExpand}
        class="w-full p-4 flex justify-between items-center hover:bg-surface-elevated/40 transition-colors cursor-pointer text-left"
    >
        <div class="flex items-center gap-2">
            <i class="fa-solid fa-chevron-right text-[10px] text-text-subtle transition-transform {expanded ? 'rotate-90' : ''}"></i>
            <span class="text-[15px] font-semibold">{skill.name}</span>
        </div>
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

            {#if !skill.enabled}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-text-subtle/10 text-text-subtle">
                    <i class="fa-solid fa-ban mr-1"></i>disabled
                </span>
            {:else if skill.running}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-success-500/15 text-success-500">
                    <i class="fa-solid fa-circle-check mr-1"></i>running
                </span>
            {:else}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-error-500/12 text-error-400">
                    <i class="fa-solid fa-circle-stop mr-1"></i>stopped
                </span>
            {/if}

            {#if hasExtension}
                <span class="text-[11px] px-2 py-0.5 rounded-full font-medium bg-violet-900/30 text-violet-400 border border-violet-800/40">
                    <i class="fa-solid fa-plug mr-1"></i>ext
                </span>
            {/if}

            {#if skill.pid}
                <span class="text-[11px] text-text-subtle font-mono">PID {skill.pid}</span>
            {/if}
        </div>
    </button>

    {#if skill.description && !expanded}
        <div class="text-sm text-text-muted px-4 pb-3 -mt-1">{skill.description}</div>
    {/if}

    <!-- Expanded detail panel -->
    {#if expanded}
        <div class="border-t border-border">
            {#if skill.description}
                <div class="text-sm text-text-muted px-4 pt-3 pb-2">{skill.description}</div>
            {/if}

            <!-- Action buttons -->
            <div class="flex items-center gap-2 px-4 py-2 border-b border-border">
                <button
                    onclick={toggleEnabled}
                    class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface transition-colors {skill.enabled ? 'hover:bg-error-500/10 hover:border-error-500 text-error-400' : 'hover:bg-success-500/10 hover:border-success-500 text-success-500'}"
                >
                    {#if skill.enabled}
                        <i class="fa-solid fa-power-off mr-1"></i>Disable
                    {:else}
                        <i class="fa-solid fa-play mr-1"></i>Enable
                    {/if}
                </button>
                <button
                    onclick={restart}
                    class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
                >
                    <i class="fa-solid fa-rotate-right mr-1"></i>Restart
                </button>
                <button
                    onclick={loadDetail}
                    class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
                >
                    <i class="fa-solid fa-arrows-rotate mr-1"></i>Refresh
                </button>
                <div class="flex-1"></div>
                <button
                    onclick={deleteSkill}
                    disabled={deleting}
                    class="px-3 py-1.5 text-xs border border-error-500/40 rounded-md bg-surface text-error-400 hover:bg-error-500/10 hover:border-error-500 transition-colors disabled:opacity-50"
                >
                    {#if deleting}
                        <i class="fa-solid fa-spinner fa-spin mr-1"></i>Deleting...
                    {:else}
                        <i class="fa-solid fa-trash-can mr-1"></i>Delete
                    {/if}
                </button>
            </div>

            {#if detailLoading}
                <div class="p-4 text-sm text-text-subtle italic text-center">
                    <i class="fa-solid fa-spinner fa-spin mr-1"></i>Loading detail...
                </div>
            {:else if detailError}
                <div class="p-4 text-sm text-error-400 italic text-center">{detailError}</div>
            {:else if detail}
                <!-- Tab bar -->
                <div class="flex border-b border-border">
                    <button
                        onclick={() => switchTab('params')}
                        class="px-4 py-2 text-xs font-semibold uppercase tracking-wider transition-colors {activeTab === 'params' ? 'text-primary-400 border-b-2 border-primary-400' : 'text-text-muted hover:text-text'}"
                    >
                        <i class="fa-solid fa-sliders mr-1"></i>Parameters
                    </button>
                    <button
                        onclick={() => switchTab('logs')}
                        class="px-4 py-2 text-xs font-semibold uppercase tracking-wider transition-colors {activeTab === 'logs' ? 'text-primary-400 border-b-2 border-primary-400' : 'text-text-muted hover:text-text'}"
                    >
                        <i class="fa-solid fa-terminal mr-1"></i>Logs
                    </button>
                    <button
                        onclick={() => switchTab('manifest')}
                        class="px-4 py-2 text-xs font-semibold uppercase tracking-wider transition-colors {activeTab === 'manifest' ? 'text-primary-400 border-b-2 border-primary-400' : 'text-text-muted hover:text-text'}"
                    >
                        <i class="fa-solid fa-file-code mr-1"></i>Manifest
                    </button>
                    {#if hasExtension}
                        <button
                            onclick={() => switchTab('plugin')}
                            class="px-4 py-2 text-xs font-semibold uppercase tracking-wider transition-colors {activeTab === 'plugin' ? 'text-violet-400 border-b-2 border-violet-400' : 'text-text-muted hover:text-text'}"
                        >
                            <i class="fa-solid fa-plug mr-1"></i>Extension
                        </button>
                    {/if}
                </div>

                <!-- Tab content -->
                <div class="p-4">
                    {#if activeTab === 'params'}
                        <!-- Skill info -->
                        <div class="grid grid-cols-2 gap-x-4 gap-y-1 mb-4 text-xs">
                            <div class="text-text-muted">Entrypoint</div>
                            <div class="font-mono text-accent-300">{detail.entrypoint}</div>
                            <div class="text-text-muted">Directory</div>
                            <div class="font-mono text-accent-300 truncate" title={detail.dir}>{detail.dir}</div>
                            <div class="text-text-muted">Type</div>
                            <div>{detail.skill_type}</div>
                            <div class="text-text-muted">Enabled</div>
                            <div>{detail.enabled ? 'Yes' : 'No'}</div>
                        </div>

                        <!-- Env vars -->
                        {#if Object.keys(detail.env).length > 0 || true}
                            <div class="border-t border-border pt-3 mb-3">
                                <div class="text-xs font-semibold uppercase tracking-wider text-primary-400 mb-2">
                                    <i class="fa-solid fa-gear mr-1"></i> Environment Variables
                                </div>
                                {#each Object.entries(detail.env) as [key, value] (key)}
                                    <div class="flex items-center gap-2 mb-1.5 text-xs">
                                        <span class="font-mono text-accent-300 min-w-[180px]">{key}</span>
                                        <span class="flex-1 font-mono text-text-muted truncate" title={value}>{value}</span>
                                        <button
                                            onclick={() => deleteEnvVar(key)}
                                            title="Remove"
                                            class="px-2 py-0.5 text-xs border border-border rounded bg-surface text-error-500 hover:bg-error-500/10 hover:border-error-500 transition-colors shrink-0"
                                        >
                                            <i class="fa-solid fa-trash-can"></i>
                                        </button>
                                    </div>
                                {/each}

                                <!-- Add new env var -->
                                <div class="flex items-center gap-2 mt-2 text-xs">
                                    <input
                                        type="text"
                                        bind:value={newEnvKey}
                                        placeholder="KEY"
                                        class="w-[180px] px-2 py-1 border border-border rounded bg-background text-text text-xs font-mono outline-none focus:border-primary-500 placeholder:text-text-subtle"
                                    />
                                    <input
                                        type="text"
                                        bind:value={newEnvValue}
                                        placeholder="value"
                                        class="flex-1 px-2 py-1 border border-border rounded bg-background text-text text-xs font-mono outline-none focus:border-primary-500 placeholder:text-text-subtle"
                                    />
                                    <button
                                        onclick={addEnvVar}
                                        class="px-2.5 py-1 text-xs border border-border rounded bg-surface text-success-500 hover:bg-success-500/10 hover:border-success-500 transition-colors shrink-0"
                                    >
                                        <i class="fa-solid fa-plus mr-1"></i>Add
                                    </button>
                                </div>
                            </div>
                        {/if}

                        <!-- Credentials -->
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

                    {:else if activeTab === 'logs'}
                        <div class="relative">
                            <div class="flex justify-between items-center mb-2">
                                <span class="text-xs text-text-muted">
                                    Last 300 lines of <span class="font-mono">skill.log</span>
                                </span>
                                <button
                                    onclick={loadLog}
                                    class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
                                >
                                    <i class="fa-solid fa-arrows-rotate mr-1"></i>Refresh
                                </button>
                            </div>
                            {#if logLoading}
                                <div class="p-4 text-sm text-text-subtle italic text-center">
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i>Loading...
                                </div>
                            {:else}
                                <pre class="bg-background border border-border rounded-md p-3 text-[11px] font-mono text-text-muted max-h-[400px] overflow-auto whitespace-pre-wrap break-all leading-relaxed">{logContent || '(no log output)'}</pre>
                            {/if}
                        </div>

                    {:else if activeTab === 'plugin' && extension}
                        <div class="space-y-4">
                            <!-- Extension UI Panel (iframe) -->
                            {#if extension.ui.panel}
                                <div>
                                    <div class="text-xs font-semibold uppercase tracking-wider text-violet-400 mb-2">
                                        <i class="fa-solid fa-window-maximize mr-1"></i> Skill Panel
                                    </div>
                                    <div class="bg-background border border-border rounded-md overflow-hidden">
                                        <iframe
                                            src="/skills/{encodeURIComponent(skill.name)}/ui/{extension.ui.panel}"
                                            class="w-full border-0"
                                            style="min-height: 300px;"
                                            title="{skill.name} extension panel"
                                            sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
                                        ></iframe>
                                    </div>
                                </div>
                            {/if}

                            <!-- Full page link -->
                            {#if extension.ui.page}
                                <div class="flex items-center gap-2">
                                    <a
                                        href="/skills/{encodeURIComponent(skill.name)}/page"
                                        target="_blank"
                                        class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-violet-800/50 bg-violet-900/20 text-violet-400 hover:bg-violet-900/40 transition-colors"
                                    >
                                        <i class="fa-solid fa-arrow-up-right-from-square"></i> Open full page
                                    </a>
                                </div>
                            {/if}

                            <!-- Registered routes -->
                            {#if extension.route_count > 0}
                                <div>
                                    <div class="text-xs font-semibold uppercase tracking-wider text-violet-400 mb-2">
                                        <i class="fa-solid fa-route mr-1"></i> Registered Routes ({extension.route_count})
                                    </div>
                                    <div class="space-y-1">
                                        {#each extension.routes as route}
                                            <div class="flex items-center gap-2 text-xs font-mono">
                                                <span class="px-1.5 py-0.5 rounded bg-violet-900/30 text-violet-300 border border-violet-800/40 min-w-[48px] text-center">
                                                    {route.split(' ')[0]}
                                                </span>
                                                <span class="text-text-muted">/api/skills/{skill.name}/ext{route.split(' ').slice(1).join(' ')}</span>
                                            </div>
                                        {/each}
                                    </div>
                                </div>
                            {/if}

                            {#if !extension.ui.panel && extension.route_count === 0}
                                <p class="text-sm text-text-subtle italic text-center py-4">
                                    This skill has an extension but no UI panel or routes registered.
                                </p>
                            {/if}
                        </div>

                    {:else if activeTab === 'manifest'}
                        <div>
                            <div class="flex justify-between items-center mb-2">
                                <span class="text-xs text-text-muted">
                                    Edit <span class="font-mono">skill.toml</span> directly
                                </span>
                                <button
                                    onclick={saveManifest}
                                    disabled={manifestSaving}
                                    class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface text-success-500 hover:bg-success-500/10 hover:border-success-500 transition-colors disabled:opacity-50"
                                >
                                    {#if manifestSaving}
                                        <i class="fa-solid fa-spinner fa-spin mr-1"></i>Saving...
                                    {:else}
                                        <i class="fa-solid fa-floppy-disk mr-1"></i>Save
                                    {/if}
                                </button>
                            </div>
                            {#if manifestError}
                                <div class="text-xs text-error-400 mb-2 p-2 bg-error-500/10 border border-error-500/30 rounded">
                                    <i class="fa-solid fa-triangle-exclamation mr-1"></i>{manifestError}
                                </div>
                            {/if}
                            {#if manifestSuccess}
                                <div class="text-xs text-success-500 mb-2 p-2 bg-success-500/10 border border-success-500/30 rounded">
                                    <i class="fa-solid fa-check mr-1"></i>{manifestSuccess}
                                </div>
                            {/if}
                            <textarea
                                bind:value={manifestEdit}
                                spellcheck="false"
                                class="w-full h-[320px] bg-background border border-border rounded-md p-3 text-[12px] font-mono text-text outline-none focus:border-primary-500 focus:ring-1 focus:ring-primary-900 resize-y leading-relaxed"
                            ></textarea>
                        </div>
                    {/if}
                </div>
            {/if}
        </div>
    {/if}
</div>
