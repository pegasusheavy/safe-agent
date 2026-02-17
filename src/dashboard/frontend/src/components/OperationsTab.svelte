<script lang="ts">
    import { onMount } from 'svelte';

    type Section = 'health' | 'update' | 'backup' | 'federation' | 'backends';
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

    onMount(() => {
        fetchHealth();
        fetchFederation();
        fetchBackends();
    });

    const sections: { id: Section; label: string; icon: string }[] = [
        { id: 'health', label: 'Health', icon: 'fa-heart-pulse' },
        { id: 'update', label: 'Updates', icon: 'fa-download' },
        { id: 'backup', label: 'Backup', icon: 'fa-box-archive' },
        { id: 'federation', label: 'Federation', icon: 'fa-network-wired' },
        { id: 'backends', label: 'LLM Backends', icon: 'fa-brain' },
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
                <i class="fa-solid fa-heart-pulse mr-1"></i> Health Check
            </h3>
            {#if healthLoading}
                <p class="text-muted">Checking health...</p>
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
                                <div class="text-xs text-muted mb-1">Database</div>
                                <div class="font-mono {health.checks.database === 'ok' ? 'text-green-400' : 'text-red-400'}">
                                    {health.checks.database}
                                </div>
                            </div>
                            <div class="bg-surface rounded p-3">
                                <div class="text-xs text-muted mb-1">Agent</div>
                                <div class="font-mono {health.checks.agent === 'running' ? 'text-green-400' : 'text-yellow-400'}">
                                    {health.checks.agent}
                                </div>
                            </div>
                            <div class="bg-surface rounded p-3">
                                <div class="text-xs text-muted mb-1">Tools</div>
                                <div class="font-mono text-fg">{health.checks.tools}</div>
                            </div>
                        </div>
                    {/if}

                    <button class="btn-secondary text-sm mt-2" onclick={fetchHealth}>
                        <i class="fa-solid fa-rotate mr-1"></i> Refresh
                    </button>
                </div>
            {/if}
        </div>

        <!-- Prometheus metrics info -->
        <div class="card">
            <h3 class="text-lg font-semibold mb-2">
                <i class="fa-solid fa-chart-bar mr-1"></i> Prometheus Metrics
            </h3>
            <p class="text-sm text-muted mb-2">
                Metrics are exposed at <code class="bg-surface px-1.5 py-0.5 rounded font-mono text-accent">/metrics</code> in Prometheus text format.
            </p>
            <p class="text-sm text-muted">
                Point your Prometheus scrape config or Grafana agent at this endpoint.
                No authentication is required for <code class="bg-surface px-1 py-0.5 rounded font-mono">/metrics</code>.
            </p>
            <div class="mt-2">
                <button class="btn-secondary text-sm" onclick={() => window.open('/metrics', '_blank')}>
                    <i class="fa-solid fa-arrow-up-right-from-square mr-1"></i> View Raw Metrics
                </button>
            </div>
        </div>
    {/if}

    <!-- Updates -->
    {#if activeSection === 'update'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-download mr-1"></i> Auto-Update
            </h3>
            <div class="space-y-3">
                {#if updateInfo}
                    <div class="flex items-center gap-3 flex-wrap">
                        <span class="text-sm text-muted">Current:</span>
                        <span class="font-mono text-fg">v{updateInfo.current_version}</span>
                        {#if updateInfo.update_available}
                            <span class="text-sm text-muted">&rarr;</span>
                            <span class="font-mono text-green-400">v{updateInfo.latest_version}</span>
                            <span class="bg-green-500/20 text-green-400 text-xs px-2 py-0.5 rounded">Update Available</span>
                        {:else}
                            <span class="bg-surface text-muted text-xs px-2 py-0.5 rounded">Up to date</span>
                        {/if}
                    </div>

                    {#if updateInfo.update_available && updateInfo.release_notes}
                        <div class="bg-surface rounded p-3 text-sm">
                            <div class="text-xs text-muted mb-1">Release Notes</div>
                            <pre class="whitespace-pre-wrap text-fg">{updateInfo.release_notes}</pre>
                        </div>
                    {/if}

                    {#if updateInfo.update_available}
                        <div class="flex gap-2">
                            <button class="btn-primary text-sm" onclick={applyUpdate} disabled={updateApplying}>
                                {#if updateApplying}
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i> Applying...
                                {:else}
                                    <i class="fa-solid fa-rocket mr-1"></i> Apply Update
                                {/if}
                            </button>
                            {#if updateInfo.release_url}
                                <a href={updateInfo.release_url} target="_blank" class="btn-secondary text-sm">
                                    <i class="fa-solid fa-arrow-up-right-from-square mr-1"></i> View Release
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
                        <i class="fa-solid fa-spinner fa-spin mr-1"></i> Checking...
                    {:else}
                        <i class="fa-solid fa-magnifying-glass mr-1"></i> Check for Updates
                    {/if}
                </button>
            </div>
        </div>
    {/if}

    <!-- Backup & Restore -->
    {#if activeSection === 'backup'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-box-archive mr-1"></i> Backup & Restore
            </h3>
            <div class="space-y-4">
                <div>
                    <h4 class="text-sm font-medium mb-2">Export Backup</h4>
                    <p class="text-xs text-muted mb-2">
                        Download a JSON export of all agent data (memory, activity, goals, cron jobs, stats).
                    </p>
                    <button class="btn-primary text-sm" onclick={downloadBackup} disabled={backupLoading}>
                        {#if backupLoading}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> Exporting...
                        {:else}
                            <i class="fa-solid fa-download mr-1"></i> Download Backup
                        {/if}
                    </button>
                </div>

                <hr class="border-border" />

                <div>
                    <h4 class="text-sm font-medium mb-2">Restore from Backup</h4>
                    <p class="text-xs text-muted mb-2">
                        Upload a previously exported backup file. Existing data will be merged (INSERT OR REPLACE).
                    </p>
                    <label class="btn-secondary text-sm inline-flex items-center cursor-pointer {restoreLoading ? 'opacity-50' : ''}">
                        {#if restoreLoading}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> Restoring...
                        {:else}
                            <i class="fa-solid fa-upload mr-1"></i> Upload Backup File
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
                <i class="fa-solid fa-network-wired mr-1"></i> Multi-Node Federation
            </h3>

            {#if fedLoading}
                <p class="text-muted">Loading federation status...</p>
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
                            Federation is not enabled. Add <code class="bg-surface px-1 py-0.5 rounded font-mono">[federation]</code> with <code class="bg-surface px-1 py-0.5 rounded font-mono">enabled = true</code> to your config to enable multi-node features.
                        </p>
                    {:else}
                        <!-- Add Peer -->
                        <div class="flex gap-2 items-end">
                            <div class="flex-1">
                                <label class="text-xs text-muted block mb-1">Add Peer Address</label>
                                <input
                                    type="text"
                                    bind:value={addPeerAddress}
                                    placeholder="http://192.168.1.101:3031"
                                    class="w-full bg-surface border border-border rounded px-3 py-1.5 text-sm font-mono"
                                />
                            </div>
                            <button class="btn-primary text-sm" onclick={addPeer} disabled={addPeerLoading}>
                                {#if addPeerLoading}
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i>
                                {:else}
                                    <i class="fa-solid fa-plus mr-1"></i>
                                {/if}
                                Add
                            </button>
                        </div>
                        {#if addPeerMessage}
                            <div class="text-sm text-muted">{addPeerMessage}</div>
                        {/if}

                        <!-- Peer List -->
                        <h4 class="text-sm font-medium">Peers ({fedStatus.peer_count || 0})</h4>
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
                                        <button class="text-red-400 hover:text-red-300 text-sm" title="Remove peer" onclick={() => removePeer(peer.node_id)}>
                                            <i class="fa-solid fa-xmark"></i>
                                        </button>
                                    </div>
                                {/each}
                            </div>
                        {:else}
                            <p class="text-sm text-muted">No peers connected.</p>
                        {/if}
                    {/if}

                    <button class="btn-secondary text-sm" onclick={fetchFederation}>
                        <i class="fa-solid fa-rotate mr-1"></i> Refresh
                    </button>
                </div>
            {/if}
        </div>
    {/if}

    <!-- LLM Backends -->
    {#if activeSection === 'backends'}
        <div class="card">
            <h3 class="text-lg font-semibold mb-3">
                <i class="fa-solid fa-brain mr-1"></i> LLM Backend Plugins
            </h3>

            {#if backendsLoading}
                <p class="text-muted">Loading backends...</p>
            {:else if backends}
                <div class="space-y-3">
                    <div class="flex items-center gap-3">
                        <span class="text-sm text-muted">Active:</span>
                        <span class="bg-accent/20 text-accent px-2 py-1 rounded font-mono text-sm">{backends.active}</span>
                        <span class="text-sm text-muted">({backends.active_info})</span>
                    </div>

                    <h4 class="text-sm font-medium mt-3">Available Backends</h4>
                    <div class="grid grid-cols-2 sm:grid-cols-3 gap-2">
                        {#each backends.available as key}
                            <div class="bg-surface rounded p-3 flex items-center gap-2 {key === backends.active ? 'ring-1 ring-accent' : ''}">
                                <i class="fa-solid {key === backends.active ? 'fa-circle-check text-accent' : 'fa-circle text-muted'} text-xs"></i>
                                <span class="font-mono text-sm">{key}</span>
                            </div>
                        {/each}
                    </div>

                    <p class="text-xs text-muted mt-2">
                        To switch backends, set <code class="bg-surface px-1 py-0.5 rounded font-mono">backend</code> in <code class="bg-surface px-1 py-0.5 rounded font-mono">[llm]</code> config or the <code class="bg-surface px-1 py-0.5 rounded font-mono">LLM_BACKEND</code> env var and restart.
                        Custom backends can be registered as plugins at runtime.
                    </p>

                    <button class="btn-secondary text-sm" onclick={fetchBackends}>
                        <i class="fa-solid fa-rotate mr-1"></i> Refresh
                    </button>
                </div>
            {/if}
        </div>
    {/if}
</div>
