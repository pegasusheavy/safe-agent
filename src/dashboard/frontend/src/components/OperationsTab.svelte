<script lang="ts">
    import { onMount } from 'svelte';
    import { t } from '../lib/i18n';

    type Section = 'health' | 'update' | 'backup' | 'federation' | 'backends' | 'advisor';
    let activeSection: Section = $state('health');

    // Health data
    let health: any = $state(null);
    let healthLoading = $state(true);

    // Update data
    let updateInfo: any = $state(null);
    let updateLoading = $state(false);
    let updateApplying = $state(false);
    let updateMessage = $state('');

    // Backup
    let backupLoading = $state(false);
    let restoreLoading = $state(false);
    let restoreMessage = $state('');

    // Federation
    let fedStatus: any = $state(null);
    let fedLoading = $state(true);
    let addPeerAddress = $state('');
    let addPeerLoading = $state(false);
    let addPeerMessage = $state('');

    // LLM Backends
    let backends: any = $state(null);
    let backendsLoading = $state(true);

    // Model Advisor
    let systemSpecs: any = $state(null);
    let specsLoading = $state(false);
    let recommendations: any[] = $state([]);
    let recsLoading = $state(false);
    let useCaseFilter = $state('');
    let ollamaStatus: any = $state(null);
    let ollamaLoading = $state(false);
    let pullingModel = $state('');
    let pullStatus = $state('');
    let configureMsg = $state('');

    async function fetchHealth() {
        healthLoading = true;
        try {
            const res = await fetch('/healthz');
            health = await res.json();
        } catch (e) {
            health = { status: 'error', checks: {} };
        }
        healthLoading = false;
    }

    async function checkForUpdate() {
        updateLoading = true;
        updateMessage = '';
        try {
            const res = await fetch('/api/update/check');
            updateInfo = await res.json();
        } catch (e) {
            updateMessage = 'Failed to check for updates';
        }
        updateLoading = false;
    }

    async function applyUpdate() {
        updateApplying = true;
        updateMessage = '';
        try {
            const res = await fetch('/api/update/apply', { method: 'POST' });
            const data = await res.json();
            updateMessage = data.message || (data.ok ? 'Update applied' : 'Update failed');
        } catch (e) {
            updateMessage = 'Failed to apply update';
        }
        updateApplying = false;
    }

    async function downloadBackup() {
        backupLoading = true;
        try {
            const res = await fetch('/api/backup');
            const blob = await res.blob();
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = `safe-agent-backup-${new Date().toISOString().slice(0,10)}.json`;
            a.click();
            URL.revokeObjectURL(url);
        } catch (e) {
            console.error('Backup download failed', e);
        }
        backupLoading = false;
    }

    async function restoreBackup(event: Event) {
        const input = event.target as HTMLInputElement;
        if (!input.files?.length) return;

        restoreLoading = true;
        restoreMessage = '';
        try {
            const text = await input.files[0].text();
            const data = JSON.parse(text);
            const res = await fetch('/api/restore', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(data),
            });
            const result = await res.json();
            restoreMessage = result.message || (result.ok ? 'Restore complete' : 'Restore failed');
        } catch (e) {
            restoreMessage = 'Failed to restore backup';
        }
        restoreLoading = false;
        input.value = '';
    }

    async function fetchFederation() {
        fedLoading = true;
        try {
            const res = await fetch('/api/federation/status');
            fedStatus = await res.json();
        } catch (e) {
            fedStatus = { enabled: false, peers: [] };
        }
        fedLoading = false;
    }

    async function addPeer() {
        if (!addPeerAddress.trim()) return;
        addPeerLoading = true;
        addPeerMessage = '';
        try {
            const res = await fetch('/api/federation/peers', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ address: addPeerAddress }),
            });
            const data = await res.json();
            addPeerMessage = data.message || (data.ok ? 'Peer added' : 'Failed');
            if (data.ok) {
                addPeerAddress = '';
                fetchFederation();
            }
        } catch (e) {
            addPeerMessage = 'Failed to add peer';
        }
        addPeerLoading = false;
    }

    async function removePeer(id: string) {
        try {
            await fetch(`/api/federation/peers/${id}`, { method: 'DELETE' });
            fetchFederation();
        } catch (e) {
            console.error('Failed to remove peer', e);
        }
    }

    async function fetchBackends() {
        backendsLoading = true;
        try {
            const res = await fetch('/api/llm/backends');
            backends = await res.json();
        } catch (e) {
            backends = { active: 'unknown', available: [] };
        }
        backendsLoading = false;
    }

    async function fetchSystemSpecs() {
        specsLoading = true;
        try {
            const res = await fetch('/api/llm/advisor/system');
            systemSpecs = await res.json();
        } catch (e) {
            systemSpecs = null;
        }
        specsLoading = false;
    }

    async function fetchRecommendations() {
        recsLoading = true;
        try {
            const params = new URLSearchParams();
            if (useCaseFilter) params.set('use_case', useCaseFilter);
            params.set('limit', '20');
            const res = await fetch(`/api/llm/advisor/recommend?${params}`);
            const data = await res.json();
            recommendations = data.models || [];
        } catch (e) {
            recommendations = [];
        }
        recsLoading = false;
    }

    async function fetchOllamaStatus() {
        ollamaLoading = true;
        try {
            const res = await fetch('/api/llm/ollama/status');
            ollamaStatus = await res.json();
        } catch (e) {
            ollamaStatus = { available: false, installed_models: [] };
        }
        ollamaLoading = false;
    }

    async function pullModel(tag: string) {
        pullingModel = tag;
        pullStatus = t('ops.advisor_pulling');
        try {
            const res = await fetch('/api/llm/ollama/pull', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ tag }),
            });
            const data = await res.json();
            if (data.ok) {
                pullStatus = t('ops.advisor_pull_complete');
                fetchOllamaStatus();
                fetchRecommendations();
            } else {
                pullStatus = data.error || 'Pull failed';
            }
        } catch (e) {
            pullStatus = 'Pull failed';
        }
        setTimeout(() => { pullingModel = ''; pullStatus = ''; }, 3000);
    }

    async function useModel(model: string) {
        configureMsg = '';
        try {
            const res = await fetch('/api/llm/ollama/configure', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ model }),
            });
            const data = await res.json();
            configureMsg = data.ok ? t('ops.advisor_configured', { model }) : (data.error || 'Failed');
            fetchBackends();
        } catch (e) {
            configureMsg = 'Configuration failed';
        }
        setTimeout(() => { configureMsg = ''; }, 5000);
    }

    onMount(() => {
        fetchHealth();
        fetchFederation();
        fetchBackends();
    });

    const sections: { id: Section; label: string; icon: string }[] = [
        { id: 'health', label: t('ops.health'), icon: 'fa-heart-pulse' },
        { id: 'update', label: t('ops.updates'), icon: 'fa-download' },
        { id: 'backup', label: t('ops.backup'), icon: 'fa-box-archive' },
        { id: 'federation', label: t('ops.federation'), icon: 'fa-network-wired' },
        { id: 'backends', label: t('ops.llm_backends'), icon: 'fa-brain' },
        { id: 'advisor', label: t('ops.model_advisor'), icon: 'fa-microchip' },
    ];
</script>

<div class="space-y-4">
    <div class="flex gap-2 mb-4 flex-wrap">
        {#each sections as s}
            <button
                class="px-3 py-1.5 rounded text-sm transition-colors {activeSection === s.id ? 'bg-accent text-white' : 'bg-surface text-muted hover:text-fg'}"
                onclick={() => activeSection = s.id}
            >
                <i class="fa-solid {s.icon} mr-1"></i> {s.label}
            </button>
        {/each}
    </div>

    <!-- Health Check -->
    {#if activeSection === 'health'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-heart-pulse mr-1"></i> {t('ops.health_check')}
            </h3>
            {#if healthLoading}
                <p class="text-muted">{t('ops.checking_health')}</p>
            {:else if health}
                <div class="space-y-3">
                    <div class="flex items-center gap-3">
                        <span class="px-3 py-1 rounded text-sm font-mono font-bold {health.status === 'healthy' ? 'bg-green-500/20 text-green-400' : 'bg-red-500/20 text-red-400'}">
                            {health.status?.toUpperCase()}
                        </span>
                        <span class="text-muted text-sm">v{health.version}</span>
                    </div>

                    {#if health.checks}
                        <div class="grid grid-cols-1 sm:grid-cols-3 gap-3 mt-3">
                            <div class="bg-surface rounded p-3">
                                <div class="text-xs text-muted mb-1">{t('ops.database')}</div>
                                <div class="font-mono {health.checks.database === 'ok' ? 'text-green-400' : 'text-red-400'}">
                                    {health.checks.database}
                                </div>
                            </div>
                            <div class="bg-surface rounded p-3">
                                <div class="text-xs text-muted mb-1">{t('ops.agent')}</div>
                                <div class="font-mono {health.checks.agent === 'running' ? 'text-green-400' : 'text-yellow-400'}">
                                    {health.checks.agent}
                                </div>
                            </div>
                            <div class="bg-surface rounded p-3">
                                <div class="text-xs text-muted mb-1">{t('ops.tools')}</div>
                                <div class="font-mono text-fg">{health.checks.tools}</div>
                            </div>
                        </div>
                    {/if}

                    <button class="btn-secondary text-sm mt-2" onclick={fetchHealth}>
                        <i class="fa-solid fa-rotate mr-1"></i> {t('common.refresh')}
                    </button>
                </div>
            {/if}
        </div>

        <!-- Prometheus metrics info -->
        <div class="card">
            <h3 class="text-lg font-semibold mb-2">
                <i class="fa-solid fa-chart-bar mr-1"></i> {t('ops.prometheus')}
            </h3>
            <p class="text-sm text-muted mb-2">
                {t('ops.prometheus_desc')}
            </p>
            <p class="text-sm text-muted">
                {t('ops.prometheus_hint')}
            </p>
            <div class="mt-2">
                <button class="btn-secondary text-sm" onclick={() => window.open('/metrics', '_blank')}>
                    <i class="fa-solid fa-arrow-up-right-from-square mr-1"></i> {t('ops.view_raw_metrics')}
                </button>
            </div>
        </div>
    {/if}

    <!-- Updates -->
    {#if activeSection === 'update'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-download mr-1"></i> {t('ops.auto_update')}
            </h3>
            <div class="space-y-3">
                {#if updateInfo}
                    <div class="flex items-center gap-3 flex-wrap">
                        <span class="text-sm text-muted">{t('ops.current_version')}</span>
                        <span class="font-mono text-fg">v{updateInfo.current_version}</span>
                        {#if updateInfo.update_available}
                            <span class="text-sm text-muted">&rarr;</span>
                            <span class="font-mono text-green-400">v{updateInfo.latest_version}</span>
                            <span class="bg-green-500/20 text-green-400 text-xs px-2 py-0.5 rounded">{t('ops.update_available')}</span>
                        {:else}
                            <span class="bg-surface text-muted text-xs px-2 py-0.5 rounded">{t('ops.up_to_date')}</span>
                        {/if}
                    </div>

                    {#if updateInfo.update_available && updateInfo.release_notes}
                        <div class="bg-surface rounded p-3 text-sm">
                            <div class="text-xs text-muted mb-1">{t('ops.release_notes')}</div>
                            <pre class="whitespace-pre-wrap text-fg">{updateInfo.release_notes}</pre>
                        </div>
                    {/if}

                    {#if updateInfo.update_available}
                        <div class="flex gap-2">
                            <button class="btn-primary text-sm" onclick={applyUpdate} disabled={updateApplying}>
                                {#if updateApplying}
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('ops.applying')}
                                {:else}
                                    <i class="fa-solid fa-rocket mr-1"></i> {t('ops.apply_update')}
                                {/if}
                            </button>
                            {#if updateInfo.release_url}
                                <a href={updateInfo.release_url} target="_blank" class="btn-secondary text-sm">
                                    <i class="fa-solid fa-arrow-up-right-from-square mr-1"></i> {t('ops.view_release')}
                                </a>
                            {/if}
                        </div>
                    {/if}
                {/if}

                {#if updateMessage}
                    <div class="bg-surface rounded p-2 text-sm text-muted">{updateMessage}</div>
                {/if}

                <button class="btn-secondary text-sm" onclick={checkForUpdate} disabled={updateLoading}>
                    {#if updateLoading}
                        <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('ops.checking')}
                    {:else}
                        <i class="fa-solid fa-magnifying-glass mr-1"></i> {t('ops.check_updates')}
                    {/if}
                </button>
            </div>
        </div>
    {/if}

    <!-- Backup & Restore -->
    {#if activeSection === 'backup'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-box-archive mr-1"></i> {t('ops.backup_restore')}
            </h3>
            <div class="space-y-4">
                <div>
                    <h4 class="text-sm font-medium mb-2">{t('ops.export_backup')}</h4>
                    <p class="text-xs text-muted mb-2">
                        Download a JSON export of all agent data (memory, activity, goals, cron jobs, stats).
                    </p>
                    <button class="btn-primary text-sm" onclick={downloadBackup} disabled={backupLoading}>
                        {#if backupLoading}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('ops.exporting')}
                        {:else}
                            <i class="fa-solid fa-download mr-1"></i> {t('ops.download_backup')}
                        {/if}
                    </button>
                </div>

                <hr class="border-border" />

                <div>
                    <h4 class="text-sm font-medium mb-2">{t('ops.restore_backup')}</h4>
                    <p class="text-xs text-muted mb-2">
                        Upload a previously exported backup file. Existing data will be merged (INSERT OR REPLACE).
                    </p>
                    <label class="btn-secondary text-sm inline-flex items-center cursor-pointer {restoreLoading ? 'opacity-50' : ''}">
                        {#if restoreLoading}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('ops.restoring')}
                        {:else}
                            <i class="fa-solid fa-upload mr-1"></i> {t('ops.upload_backup')}
                        {/if}
                        <input type="file" accept=".json" onchange={restoreBackup} class="hidden" disabled={restoreLoading} />
                    </label>
                    {#if restoreMessage}
                        <div class="bg-surface rounded p-2 text-sm text-muted mt-2">{restoreMessage}</div>
                    {/if}
                </div>
            </div>
        </div>
    {/if}

    <!-- Federation -->
    {#if activeSection === 'federation'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-network-wired mr-1"></i> {t('ops.federation_title')}
            </h3>

            {#if fedLoading}
                <p class="text-muted">{t('ops.federation_loading')}</p>
            {:else if fedStatus}
                <div class="space-y-4">
                    <div class="flex items-center gap-3">
                        <span class="px-2 py-1 rounded text-xs font-semibold {fedStatus.enabled ? 'bg-green-500/20 text-green-400' : 'bg-surface text-muted'}">
                            {fedStatus.enabled ? 'ENABLED' : 'DISABLED'}
                        </span>
                        {#if fedStatus.node}
                            <span class="text-sm text-muted">Node:</span>
                            <span class="font-mono text-sm text-fg">{fedStatus.node.name}</span>
                            <span class="text-xs text-muted font-mono">({fedStatus.node.node_id.slice(0, 8)}...)</span>
                        {/if}
                    </div>

                    {#if !fedStatus.enabled}
                        <p class="text-sm text-muted">
                            {t('ops.federation_disabled')}
                        </p>
                    {:else}
                        <!-- Add Peer -->
                        <div class="flex gap-2 items-end">
                            <div class="flex-1">
                                <label class="text-xs text-muted block mb-1">{t('ops.add_peer')}</label>
                                <input
                                    type="text"
                                    bind:value={addPeerAddress}
                                    placeholder={t('ops.peer_placeholder')}
                                    class="w-full bg-surface border border-border rounded px-3 py-1.5 text-sm font-mono"
                                />
                            </div>
                            <button class="btn-primary text-sm" onclick={addPeer} disabled={addPeerLoading}>
                                {#if addPeerLoading}
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i>
                                {:else}
                                    <i class="fa-solid fa-plus mr-1"></i>
                                {/if}
                                {t('ops.add')}
                            </button>
                        </div>
                        {#if addPeerMessage}
                            <div class="text-sm text-muted">{addPeerMessage}</div>
                        {/if}

                        <!-- Peer List -->
                        <h4 class="text-sm font-medium">{t('ops.peers_count', { count: fedStatus.peer_count || 0 })}</h4>
                        {#if fedStatus.peers?.length}
                            <div class="space-y-2">
                                {#each fedStatus.peers as peer}
                                    <div class="bg-surface rounded p-3 flex items-center justify-between">
                                        <div>
                                            <span class="font-mono text-sm text-fg">{peer.name}</span>
                                            <span class="text-xs text-muted ml-2">{peer.address}</span>
                                            <span class="ml-2 px-1.5 py-0.5 rounded text-xs {peer.status === 'online' ? 'bg-green-500/20 text-green-400' : 'bg-red-500/20 text-red-400'}">
                                                {peer.status}
                                            </span>
                                            <span class="text-xs text-muted ml-2">v{peer.version}</span>
                                        </div>
                                        <button class="text-red-400 hover:text-red-300 text-sm" title={t('ops.remove_peer_title')} onclick={() => removePeer(peer.node_id)}>
                                            <i class="fa-solid fa-xmark"></i>
                                        </button>
                                    </div>
                                {/each}
                            </div>
                        {:else}
                            <p class="text-sm text-muted">{t('ops.no_peers')}</p>
                        {/if}
                    {/if}

                    <button class="btn-secondary text-sm" onclick={fetchFederation}>
                        <i class="fa-solid fa-rotate mr-1"></i> {t('common.refresh')}
                    </button>
                </div>
            {/if}
        </div>
    {/if}

    <!-- LLM Backends -->
    {#if activeSection === 'backends'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-brain mr-1"></i> {t('ops.llm_plugins')}
            </h3>

            {#if backendsLoading}
                <p class="text-muted">{t('ops.loading_backends')}</p>
            {:else if backends}
                <div class="space-y-3">
                    <div class="flex items-center gap-3">
                        <span class="text-sm text-muted">{t('ops.active_backend')}</span>
                        <span class="bg-accent/20 text-accent px-2 py-1 rounded font-mono text-sm">{backends.active}</span>
                        <span class="text-sm text-muted">({backends.active_info})</span>
                    </div>

                    <h4 class="text-sm font-medium mt-3">{t('ops.available_backends')}</h4>
                    <div class="grid grid-cols-2 sm:grid-cols-3 gap-2">
                        {#each backends.available as key}
                            <div class="bg-surface rounded p-3 flex items-center gap-2 {key === backends.active ? 'ring-1 ring-accent' : ''}">
                                <i class="fa-solid {key === backends.active ? 'fa-circle-check text-accent' : 'fa-circle text-muted'} text-xs"></i>
                                <span class="font-mono text-sm">{key}</span>
                            </div>
                        {/each}
                    </div>

                    <p class="text-xs text-muted mt-2">
                        {t('ops.switch_backend_hint')}
                    </p>

                    <button class="btn-secondary text-sm" onclick={fetchBackends}>
                        <i class="fa-solid fa-rotate mr-1"></i> {t('common.refresh')}
                    </button>
                </div>
            {/if}
        </div>
    {/if}

    <!-- Model Advisor -->
    {#if activeSection === 'advisor'}
        <!-- System Specs -->
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-microchip mr-1"></i> {t('ops.system_specs')}
            </h3>
            {#if specsLoading}
                <p class="text-muted">{t('common.loading')}</p>
            {:else if systemSpecs}
                <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
                    <div class="bg-surface rounded p-3">
                        <div class="text-xs text-muted mb-1">{t('ops.advisor_ram')}</div>
                        <div class="font-mono text-fg">{systemSpecs.total_ram_gb?.toFixed(1)} GB</div>
                        <div class="text-xs text-muted">{systemSpecs.available_ram_gb?.toFixed(1)} GB {t('ops.advisor_available')}</div>
                    </div>
                    <div class="bg-surface rounded p-3">
                        <div class="text-xs text-muted mb-1">{t('ops.advisor_cpu')}</div>
                        <div class="font-mono text-fg">{systemSpecs.cpu_cores} {t('ops.advisor_cores')}</div>
                        <div class="text-xs text-muted truncate" title={systemSpecs.cpu_name}>{systemSpecs.cpu_name}</div>
                    </div>
                    <div class="bg-surface rounded p-3">
                        <div class="text-xs text-muted mb-1">{t('ops.advisor_gpu')}</div>
                        {#if systemSpecs.has_gpu}
                            <div class="font-mono text-fg">{systemSpecs.gpu_vram_gb?.toFixed(1) ?? '?'} GB VRAM</div>
                            <div class="text-xs text-muted truncate" title={systemSpecs.gpu_name}>{systemSpecs.gpu_name || 'GPU'}{systemSpecs.gpu_count > 1 ? ` (×${systemSpecs.gpu_count})` : ''}</div>
                        {:else}
                            <div class="font-mono text-muted">{t('ops.advisor_no_gpu')}</div>
                        {/if}
                    </div>
                    <div class="bg-surface rounded p-3">
                        <div class="text-xs text-muted mb-1">{t('ops.advisor_backend_accel')}</div>
                        <div class="font-mono text-fg">{systemSpecs.backend}</div>
                        {#if systemSpecs.unified_memory}
                            <div class="text-xs text-green-400">{t('ops.advisor_unified')}</div>
                        {/if}
                    </div>
                </div>
            {:else}
                <button class="btn-primary text-sm" onclick={() => { fetchSystemSpecs(); fetchRecommendations(); fetchOllamaStatus(); }}>
                    <i class="fa-solid fa-wand-magic-sparkles mr-1"></i> {t('ops.advisor_detect')}
                </button>
            {/if}
        </div>

        <!-- Ollama Status -->
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-server mr-1"></i> {t('ops.ollama_status')}
            </h3>
            {#if ollamaLoading}
                <p class="text-muted">{t('common.loading')}</p>
            {:else if ollamaStatus}
                <div class="flex items-center gap-3 mb-3">
                    <span class="w-3 h-3 rounded-full {ollamaStatus.available ? 'bg-green-500' : 'bg-red-500'}"></span>
                    <span class="text-sm">{ollamaStatus.available ? t('ops.ollama_running') : t('ops.ollama_not_running')}</span>
                    {#if ollamaStatus.available && ollamaStatus.installed_models?.length}
                        <span class="text-xs text-muted">({ollamaStatus.installed_models.length} {t('ops.advisor_models_installed')})</span>
                    {/if}
                </div>
                {#if ollamaStatus.available && ollamaStatus.installed_models?.length}
                    <div class="flex flex-wrap gap-2">
                        {#each ollamaStatus.installed_models as m}
                            <span class="bg-surface px-2 py-1 rounded text-xs font-mono">{m}</span>
                        {/each}
                    </div>
                {/if}
                {#if !ollamaStatus.available}
                    <p class="text-xs text-muted mt-2">{t('ops.ollama_install_hint')}</p>
                {/if}
            {:else}
                <p class="text-muted text-sm">{t('ops.advisor_detect_first')}</p>
            {/if}
        </div>

        {#if configureMsg}
            <div class="bg-green-500/10 border border-green-500/30 rounded p-3 text-sm text-green-400">
                {configureMsg}
            </div>
        {/if}

        <!-- Recommended Models -->
        {#if systemSpecs}
            <div class="card">
                <div class="flex items-center justify-between mb-3 flex-wrap gap-2">
                    <h3 class="text-lg font-semibold">
                        <i class="fa-solid fa-ranking-star mr-1"></i> {t('ops.recommended_models')}
                    </h3>
                    <div class="flex items-center gap-2">
                        <select
                            bind:value={useCaseFilter}
                            onchange={() => fetchRecommendations()}
                            class="bg-surface border border-border rounded px-2 py-1 text-sm"
                        >
                            <option value="">{t('ops.advisor_all_use_cases')}</option>
                            <option value="general">{t('ops.advisor_uc_general')}</option>
                            <option value="coding">{t('ops.advisor_uc_coding')}</option>
                            <option value="reasoning">{t('ops.advisor_uc_reasoning')}</option>
                            <option value="chat">{t('ops.advisor_uc_chat')}</option>
                            <option value="multimodal">{t('ops.advisor_uc_multimodal')}</option>
                        </select>
                        <button class="btn-secondary text-sm" onclick={fetchRecommendations}>
                            <i class="fa-solid fa-rotate mr-1"></i> {t('common.refresh')}
                        </button>
                    </div>
                </div>

                {#if recsLoading}
                    <p class="text-muted">{t('ops.advisor_analyzing')}</p>
                {:else if recommendations.length}
                    <div class="overflow-x-auto">
                        <table class="w-full text-sm">
                            <thead>
                                <tr class="text-left text-xs text-muted border-b border-border">
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_name')}</th>
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_params')}</th>
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_score')}</th>
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_fit')}</th>
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_mode')}</th>
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_quant')}</th>
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_tps')}</th>
                                    <th class="pb-2 pr-3">{t('ops.advisor_col_mem')}</th>
                                    <th class="pb-2">{t('ops.advisor_col_action')}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {#each recommendations as rec}
                                    <tr class="border-b border-border/30 hover:bg-surface/50">
                                        <td class="py-2 pr-3">
                                            <div class="font-mono text-fg">{rec.name}</div>
                                            <div class="text-xs text-muted">{rec.provider} · {rec.use_case}</div>
                                        </td>
                                        <td class="py-2 pr-3 font-mono text-xs">{rec.params_b?.toFixed(1)}B</td>
                                        <td class="py-2 pr-3">
                                            <div class="flex items-center gap-1">
                                                <div class="w-16 h-2 bg-surface rounded-full overflow-hidden">
                                                    <div
                                                        class="h-full rounded-full {rec.score >= 70 ? 'bg-green-500' : rec.score >= 40 ? 'bg-yellow-500' : 'bg-red-500'}"
                                                        style="width: {Math.min(rec.score, 100)}%"
                                                    ></div>
                                                </div>
                                                <span class="text-xs font-mono">{rec.score?.toFixed(0)}</span>
                                            </div>
                                        </td>
                                        <td class="py-2 pr-3">
                                            <span class="px-1.5 py-0.5 rounded text-xs {rec.fit_level === 'Perfect' ? 'bg-green-500/20 text-green-400' : rec.fit_level === 'Good' ? 'bg-blue-500/20 text-blue-400' : 'bg-yellow-500/20 text-yellow-400'}">
                                                {rec.fit_level}
                                            </span>
                                        </td>
                                        <td class="py-2 pr-3 text-xs">{rec.run_mode}</td>
                                        <td class="py-2 pr-3 font-mono text-xs">{rec.best_quant}</td>
                                        <td class="py-2 pr-3 font-mono text-xs">{rec.estimated_tps?.toFixed(1)}</td>
                                        <td class="py-2 pr-3 font-mono text-xs">{rec.memory_required_gb?.toFixed(1)} GB</td>
                                        <td class="py-2">
                                            {#if rec.installed}
                                                <button
                                                    class="bg-accent/20 text-accent px-2 py-1 rounded text-xs hover:bg-accent/30 transition-colors"
                                                    onclick={() => useModel(rec.ollama_tag || rec.name)}
                                                >
                                                    {t('ops.advisor_use')}
                                                </button>
                                            {:else if rec.ollama_tag && ollamaStatus?.available}
                                                {#if pullingModel === rec.ollama_tag}
                                                    <span class="text-xs text-muted">
                                                        <i class="fa-solid fa-spinner fa-spin mr-1"></i>{pullStatus}
                                                    </span>
                                                {:else}
                                                    <button
                                                        class="bg-surface text-fg px-2 py-1 rounded text-xs hover:bg-surface-elevated transition-colors"
                                                        onclick={() => pullModel(rec.ollama_tag)}
                                                    >
                                                        <i class="fa-solid fa-download mr-1"></i>{t('ops.advisor_install')}
                                                    </button>
                                                {/if}
                                            {:else}
                                                <span class="text-xs text-muted">—</span>
                                            {/if}
                                        </td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                {:else}
                    <p class="text-muted text-sm">{t('ops.advisor_no_models')}</p>
                {/if}
            </div>
        {/if}
    {/if}
</div>
