<script lang="ts">
    import { t } from '../lib/i18n';
    import { dashboard } from '../lib/state.svelte';
    import { formatDateTime } from '../lib/time';

    interface AuditEntry {
        id: number;
        event_type: string;
        tool: string | null;
        action: string | null;
        user_context: string | null;
        reasoning: string | null;
        params_json: string | null;
        result: string | null;
        success: boolean | null;
        source: string;
        created_at: string;
    }

    interface AuditSummary {
        total_events: number;
        tool_calls: number;
        approvals: number;
        rejections: number;
        rate_limits: number;
        pii_detections: number;
        twofa_challenges: number;
        permission_denials: number;
    }

    interface CostSummary {
        today_usd: number;
        today_tokens: number;
        month_usd: number;
        total_usd: number;
        total_tokens: number;
        today_requests: number;
        daily_limit_usd: number;
        limit_exceeded: boolean;
    }

    interface RateStatus {
        calls_last_minute: number;
        calls_last_hour: number;
        limit_per_minute: number;
        limit_per_hour: number;
        is_limited: boolean;
    }

    interface TwoFaChallenge {
        id: string;
        tool: string;
        description: string;
        source: string;
        age_secs: number;
        confirmed: boolean;
    }

    let auditSummary: AuditSummary | null = $state(null);
    let costSummary: CostSummary | null = $state(null);
    let rateStatus: RateStatus | null = $state(null);
    let challenges: TwoFaChallenge[] = $state([]);
    let auditEntries: AuditEntry[] = $state([]);
    let explanationChain: AuditEntry[] = $state([]);
    let showExplanation = $state(false);

    let activeSection = $state<'overview' | 'audit' | 'cost' | 'rate' | '2fa'>('overview');
    let auditFilter = $state({ event_type: '', tool: '' });

    async function loadOverview() {
        try {
            const res = await fetch('/api/security/overview');
            const data = await res.json();
            auditSummary = data.audit;
            costSummary = data.cost;
            rateStatus = data.rate_limit;
            challenges = [];
        } catch (e) {
            console.error('Failed to load security overview', e);
        }
    }

    async function loadAudit() {
        try {
            const params = new URLSearchParams({ limit: '50' });
            if (auditFilter.event_type) params.set('event_type', auditFilter.event_type);
            if (auditFilter.tool) params.set('tool', auditFilter.tool);
            const res = await fetch(`/api/security/audit?${params}`);
            auditEntries = await res.json();
        } catch (e) {
            console.error('Failed to load audit log', e);
        }
    }

    async function loadCost() {
        try {
            const res = await fetch('/api/security/cost');
            costSummary = await res.json();
        } catch (e) {
            console.error('Failed to load cost summary', e);
        }
    }

    async function loadRate() {
        try {
            const res = await fetch('/api/security/rate-limit');
            rateStatus = await res.json();
        } catch (e) {
            console.error('Failed to load rate limit status', e);
        }
    }

    async function load2FA() {
        try {
            const res = await fetch('/api/security/2fa');
            challenges = await res.json();
        } catch (e) {
            console.error('Failed to load 2FA challenges', e);
        }
    }

    async function confirm2FA(id: string) {
        await fetch(`/api/security/2fa/${id}/confirm`, { method: 'POST' });
        await load2FA();
    }

    async function reject2FA(id: string) {
        await fetch(`/api/security/2fa/${id}/reject`, { method: 'POST' });
        await load2FA();
    }

    async function explain(auditId: number) {
        try {
            const res = await fetch(`/api/security/audit/${auditId}/explain`);
            explanationChain = await res.json();
            showExplanation = true;
        } catch (e) {
            console.error('Failed to load explanation', e);
        }
    }

    function switchSection(s: typeof activeSection) {
        activeSection = s;
        if (s === 'overview') loadOverview();
        else if (s === 'audit') loadAudit();
        else if (s === 'cost') loadCost();
        else if (s === 'rate') loadRate();
        else if (s === '2fa') load2FA();
    }

    function formatUsd(n: number): string {
        return `$${n.toFixed(4)}`;
    }

    function formatTokens(n: number): string {
        if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
        if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
        return n.toString();
    }

    function eventTypeColor(t: string): string {
        switch (t) {
            case 'tool_call': return 'text-blue-400';
            case 'approval': return 'text-yellow-400';
            case 'rate_limit': return 'text-orange-400';
            case 'pii_detected': return 'text-red-400';
            case '2fa': return 'text-purple-400';
            case 'permission_denied': return 'text-red-500';
            default: return 'text-muted';
        }
    }

    function eventTypeIcon(t: string): string {
        switch (t) {
            case 'tool_call': return 'fa-wrench';
            case 'approval': return 'fa-check-double';
            case 'rate_limit': return 'fa-gauge-high';
            case 'pii_detected': return 'fa-user-shield';
            case '2fa': return 'fa-lock';
            case 'permission_denied': return 'fa-ban';
            default: return 'fa-circle-info';
        }
    }

    $effect(() => {
        // Reload on global refresh
        dashboard.refreshCounter;
        loadOverview();
    });
</script>

<div class="space-y-4">
    <div class="flex items-center gap-3 mb-4">
        <h2 class="text-xl font-bold text-heading"><i class="fa-solid fa-shield-halved mr-2"></i>{t('security.title')}</h2>
    </div>

    <!-- Sub-nav -->
    <div class="flex gap-1 border-b border-border pb-1 mb-4">
        {#each [
            { id: 'overview' as const, label: t('security.overview'), icon: 'fa-chart-pie' },
            { id: 'audit' as const, label: t('security.audit_trail'), icon: 'fa-scroll' },
            { id: 'cost' as const, label: t('security.cost_tracking'), icon: 'fa-dollar-sign' },
            { id: 'rate' as const, label: t('security.rate_limits'), icon: 'fa-gauge-high' },
            { id: '2fa' as const, label: t('security.twofa'), icon: 'fa-lock' },
        ] as section}
            <button
                class="px-3 py-1.5 text-sm rounded-t transition-colors"
                class:bg-card={activeSection === section.id}
                class:text-heading={activeSection === section.id}
                class:text-muted={activeSection !== section.id}
                class:hover:text-heading={activeSection !== section.id}
                onclick={() => switchSection(section.id)}
            >
                <i class="fa-solid {section.icon} mr-1"></i>{section.label}
            </button>
        {/each}
    </div>

    <!-- Overview Section -->
    {#if activeSection === 'overview'}
        {#if auditSummary && costSummary && rateStatus}
            <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
                <div class="card p-3 text-center">
                    <div class="text-2xl font-bold text-heading">{auditSummary.total_events}</div>
                    <div class="text-xs text-muted">{t('security.total_events')}</div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-2xl font-bold text-blue-400">{auditSummary.tool_calls}</div>
                    <div class="text-xs text-muted">{t('security.tool_calls')}</div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-2xl font-bold" class:text-red-400={costSummary.limit_exceeded} class:text-green-400={!costSummary.limit_exceeded}>
                        {formatUsd(costSummary.today_usd)}
                    </div>
                    <div class="text-xs text-muted">
                        {t('security.todays_cost')}
                        {#if costSummary.daily_limit_usd > 0}
                            / {formatUsd(costSummary.daily_limit_usd)}
                        {/if}
                    </div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-2xl font-bold" class:text-orange-400={rateStatus.is_limited} class:text-green-400={!rateStatus.is_limited}>
                        {rateStatus.calls_last_minute}/{rateStatus.limit_per_minute || '∞'}
                    </div>
                    <div class="text-xs text-muted">{t('security.calls_per_min')}</div>
                </div>
            </div>

            <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mt-3">
                <div class="card p-3 text-center">
                    <div class="text-lg font-bold text-green-400">{auditSummary.approvals}</div>
                    <div class="text-xs text-muted">{t('security.approvals')}</div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-lg font-bold text-red-400">{auditSummary.rejections}</div>
                    <div class="text-xs text-muted">{t('security.rejections')}</div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-lg font-bold text-orange-400">{auditSummary.rate_limits}</div>
                    <div class="text-xs text-muted">{t('security.rate_limits')}</div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-lg font-bold text-red-500">{auditSummary.pii_detections}</div>
                    <div class="text-xs text-muted">{t('security.pii_detections')}</div>
                </div>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-3 mt-3">
                <div class="card p-4">
                    <h3 class="text-sm font-semibold text-heading mb-2"><i class="fa-solid fa-dollar-sign mr-1"></i>Cost Summary</h3>
                    <div class="space-y-1 text-sm">
                        <div class="flex justify-between"><span class="text-muted">{t('security.todays_cost')}</span><span>{formatUsd(costSummary.today_usd)}</span></div>
                        <div class="flex justify-between"><span class="text-muted">{t('security.month_cost')}</span><span>{formatUsd(costSummary.month_usd)}</span></div>
                        <div class="flex justify-between"><span class="text-muted">{t('security.total_cost')}</span><span>{formatUsd(costSummary.total_usd)}</span></div>
                        <div class="flex justify-between"><span class="text-muted">{t('security.tokens')}</span><span>{formatTokens(costSummary.today_tokens)}</span></div>
                        <div class="flex justify-between"><span class="text-muted">{t('security.total_tokens')}</span><span>{formatTokens(costSummary.total_tokens)}</span></div>
                        <div class="flex justify-between"><span class="text-muted">Requests Today</span><span>{costSummary.today_requests}</span></div>
                    </div>
                </div>
                <div class="card p-4">
                    <h3 class="text-sm font-semibold text-heading mb-2"><i class="fa-solid fa-gauge-high mr-1"></i>Rate Limits</h3>
                    <div class="space-y-1 text-sm">
                        <div class="flex justify-between"><span class="text-muted">Per Minute</span><span>{rateStatus.calls_last_minute} / {rateStatus.limit_per_minute || '∞'}</span></div>
                        <div class="flex justify-between"><span class="text-muted">Per Hour</span><span>{rateStatus.calls_last_hour} / {rateStatus.limit_per_hour || '∞'}</span></div>
                        <div class="flex justify-between">
                            <span class="text-muted">Status</span>
                            <span class:text-red-400={rateStatus.is_limited} class:text-green-400={!rateStatus.is_limited}>
                                {rateStatus.is_limited ? 'RATE LIMITED' : 'OK'}
                            </span>
                        </div>
                        <div class="flex justify-between"><span class="text-muted">2FA Challenges</span><span>{auditSummary.twofa_challenges}</span></div>
                        <div class="flex justify-between"><span class="text-muted">Permission Denials</span><span>{auditSummary.permission_denials}</span></div>
                    </div>
                </div>
            </div>
        {:else}
            <p class="text-muted text-sm">{t('common.loading')}</p>
        {/if}

    <!-- Audit Trail Section -->
    {:else if activeSection === 'audit'}
        <div class="flex gap-2 mb-3">
            <select class="bg-bg border border-border rounded px-2 py-1 text-sm" bind:value={auditFilter.event_type} onchange={() => loadAudit()}>
                <option value="">{t('security.all_types')}</option>
                <option value="tool_call">Tool Calls</option>
                <option value="approval">Approvals</option>
                <option value="rate_limit">Rate Limits</option>
                <option value="pii_detected">PII Detected</option>
                <option value="2fa">2FA</option>
                <option value="permission_denied">Permission Denied</option>
            </select>
            <input
                type="text"
                placeholder={t('security.filter')}
                class="bg-bg border border-border rounded px-2 py-1 text-sm flex-1"
                bind:value={auditFilter.tool}
                onkeyup={(e) => { if ((e as KeyboardEvent).key === 'Enter') loadAudit(); }}
            />
            <button class="btn btn-sm" onclick={() => loadAudit()}>
                <i class="fa-solid fa-rotate mr-1"></i>{t('common.refresh')}
            </button>
        </div>

        {#if auditEntries.length === 0}
            <p class="text-muted text-sm">{t('security.no_events')}</p>
        {:else}
            <div class="space-y-1">
                {#each auditEntries as entry}
                    <div class="card p-2 flex items-start gap-2 text-sm">
                        <i class="fa-solid {eventTypeIcon(entry.event_type)} {eventTypeColor(entry.event_type)} mt-0.5"></i>
                        <div class="flex-1 min-w-0">
                            <div class="flex items-center gap-2">
                                <span class="font-semibold {eventTypeColor(entry.event_type)}">{entry.event_type}</span>
                                {#if entry.tool}
                                    <span class="text-xs bg-card-hover px-1.5 py-0.5 rounded">{entry.tool}</span>
                                {/if}
                                {#if entry.action}
                                    <span class="text-xs text-muted">{entry.action}</span>
                                {/if}
                                {#if entry.success !== null}
                                    <span class="text-xs" class:text-green-400={entry.success} class:text-red-400={!entry.success}>
                                        {entry.success ? '✓' : '✗'}
                                    </span>
                                {/if}
                                <span class="text-xs text-muted ml-auto">{entry.source}</span>
                            </div>
                            {#if entry.reasoning}
                                <div class="text-xs text-muted mt-0.5 truncate">Reasoning: {entry.reasoning}</div>
                            {/if}
                            {#if entry.result}
                                <div class="text-xs text-muted mt-0.5 truncate">Result: {entry.result}</div>
                            {/if}
                            <div class="text-xs text-muted mt-0.5 flex justify-between">
                                <span>{formatDateTime(entry.created_at)}</span>
                                <button class="text-blue-400 hover:text-blue-300" onclick={() => explain(entry.id)} title="Explain this action">
                                    <i class="fa-solid fa-magnifying-glass"></i> {t('security.why')}
                                </button>
                            </div>
                        </div>
                    </div>
                {/each}
            </div>
        {/if}

        <!-- Explanation modal -->
        {#if showExplanation}
            <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onclick={() => showExplanation = false} role="dialog">
                <div class="card p-4 max-w-lg w-full mx-4 max-h-[80vh] overflow-y-auto" onclick={(e) => e.stopPropagation()} role="document">
                    <div class="flex justify-between items-center mb-3">
                        <h3 class="text-lg font-bold text-heading"><i class="fa-solid fa-lightbulb mr-2"></i>{t('security.action_explanation')}</h3>
                        <button class="text-muted hover:text-heading" onclick={() => showExplanation = false} title={t('common.close')}>
                            <i class="fa-solid fa-xmark"></i>
                        </button>
                    </div>
                    {#if explanationChain.length === 0}
                        <p class="text-muted text-sm">{t('security.no_explanation')}</p>
                    {:else}
                        <div class="space-y-2">
                            {#each explanationChain as step, i}
                                <div class="border-l-2 pl-3" class:border-blue-400={step.event_type === 'tool_call'} class:border-yellow-400={step.event_type === 'approval'} class:border-red-400={step.event_type === 'pii_detected' || step.event_type === 'permission_denied'} class:border-border={!['tool_call','approval','pii_detected','permission_denied'].includes(step.event_type)}>
                                    <div class="text-sm font-semibold {eventTypeColor(step.event_type)}">
                                        Step {i + 1}: {step.event_type}
                                        {#if step.tool}<span class="text-muted"> ({step.tool})</span>{/if}
                                    </div>
                                    {#if step.reasoning}
                                        <div class="text-sm mt-1"><span class="text-muted">Reasoning:</span> {step.reasoning}</div>
                                    {/if}
                                    {#if step.user_context}
                                        <div class="text-xs text-muted mt-1">Context: {step.user_context}</div>
                                    {/if}
                                    {#if step.result}
                                        <div class="text-xs text-muted mt-1">Result: {step.result}</div>
                                    {/if}
                                    <div class="text-xs text-muted">{formatDateTime(step.created_at)}</div>
                                </div>
                            {/each}
                        </div>
                    {/if}
                </div>
            </div>
        {/if}

    <!-- Cost Tracking Section -->
    {:else if activeSection === 'cost'}
        {#if costSummary}
            <div class="grid grid-cols-2 md:grid-cols-3 gap-3 mb-4">
                <div class="card p-4 text-center">
                    <div class="text-3xl font-bold" class:text-red-400={costSummary.limit_exceeded} class:text-green-400={!costSummary.limit_exceeded}>
                        {formatUsd(costSummary.today_usd)}
                    </div>
                    <div class="text-xs text-muted">{t('security.todays_cost')}</div>
                    {#if costSummary.daily_limit_usd > 0}
                        <div class="mt-2 w-full bg-bg rounded-full h-2">
                            <div
                                class="h-2 rounded-full transition-all"
                                class:bg-green-400={!costSummary.limit_exceeded}
                                class:bg-red-400={costSummary.limit_exceeded}
                                style="width: {Math.min(100, (costSummary.today_usd / costSummary.daily_limit_usd) * 100)}%"
                            ></div>
                        </div>
                        <div class="text-xs text-muted mt-1">Limit: {formatUsd(costSummary.daily_limit_usd)}</div>
                    {/if}
                </div>
                <div class="card p-4 text-center">
                    <div class="text-3xl font-bold text-heading">{formatUsd(costSummary.month_usd)}</div>
                    <div class="text-xs text-muted">{t('security.month_cost')}</div>
                </div>
                <div class="card p-4 text-center">
                    <div class="text-3xl font-bold text-heading">{formatUsd(costSummary.total_usd)}</div>
                    <div class="text-xs text-muted">{t('security.total_cost')}</div>
                </div>
            </div>
            <div class="grid grid-cols-3 gap-3">
                <div class="card p-3 text-center">
                    <div class="text-xl font-bold">{formatTokens(costSummary.today_tokens)}</div>
                    <div class="text-xs text-muted">{t('security.tokens')}</div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-xl font-bold">{formatTokens(costSummary.total_tokens)}</div>
                    <div class="text-xs text-muted">{t('security.total_tokens')}</div>
                </div>
                <div class="card p-3 text-center">
                    <div class="text-xl font-bold">{costSummary.today_requests}</div>
                    <div class="text-xs text-muted">Requests Today</div>
                </div>
            </div>
        {:else}
            <p class="text-muted text-sm">{t('common.loading')}</p>
        {/if}

    <!-- Rate Limits Section -->
    {:else if activeSection === 'rate'}
        {#if rateStatus}
            <div class="card p-6 max-w-md">
                <h3 class="text-lg font-bold text-heading mb-4"><i class="fa-solid fa-gauge-high mr-2"></i>Rate Limit Status</h3>
                <div class="space-y-4">
                    <div>
                        <div class="flex justify-between text-sm mb-1">
                            <span class="text-muted">Per Minute</span>
                            <span>{rateStatus.calls_last_minute} / {rateStatus.limit_per_minute || '∞'}</span>
                        </div>
                        {#if rateStatus.limit_per_minute > 0}
                            <div class="w-full bg-bg rounded-full h-3">
                                <div
                                    class="h-3 rounded-full transition-all"
                                    class:bg-green-400={(rateStatus.calls_last_minute / rateStatus.limit_per_minute) < 0.8}
                                    class:bg-yellow-400={(rateStatus.calls_last_minute / rateStatus.limit_per_minute) >= 0.8 && (rateStatus.calls_last_minute / rateStatus.limit_per_minute) < 1}
                                    class:bg-red-400={(rateStatus.calls_last_minute / rateStatus.limit_per_minute) >= 1}
                                    style="width: {Math.min(100, (rateStatus.calls_last_minute / rateStatus.limit_per_minute) * 100)}%"
                                ></div>
                            </div>
                        {/if}
                    </div>
                    <div>
                        <div class="flex justify-between text-sm mb-1">
                            <span class="text-muted">Per Hour</span>
                            <span>{rateStatus.calls_last_hour} / {rateStatus.limit_per_hour || '∞'}</span>
                        </div>
                        {#if rateStatus.limit_per_hour > 0}
                            <div class="w-full bg-bg rounded-full h-3">
                                <div
                                    class="h-3 rounded-full transition-all"
                                    class:bg-green-400={(rateStatus.calls_last_hour / rateStatus.limit_per_hour) < 0.8}
                                    class:bg-yellow-400={(rateStatus.calls_last_hour / rateStatus.limit_per_hour) >= 0.8 && (rateStatus.calls_last_hour / rateStatus.limit_per_hour) < 1}
                                    class:bg-red-400={(rateStatus.calls_last_hour / rateStatus.limit_per_hour) >= 1}
                                    style="width: {Math.min(100, (rateStatus.calls_last_hour / rateStatus.limit_per_hour) * 100)}%"
                                ></div>
                            </div>
                        {/if}
                    </div>
                    <div class="text-center p-3 rounded" style="background-color: {rateStatus.is_limited ? 'rgba(248,113,113,0.1)' : 'rgba(74,222,128,0.1)'}">
                        <span class="text-lg font-bold" class:text-green-400={!rateStatus.is_limited} class:text-red-400={rateStatus.is_limited}>
                            {rateStatus.is_limited ? 'RATE LIMITED' : 'Within Limits'}
                        </span>
                    </div>
                </div>
            </div>
        {:else}
            <p class="text-muted text-sm">{t('common.loading')}</p>
        {/if}

    <!-- 2FA Section -->
    {:else if activeSection === '2fa'}
        <div class="space-y-3">
            <div class="flex items-center justify-between mb-2">
                <h3 class="text-lg font-bold text-heading"><i class="fa-solid fa-lock mr-2"></i>{t('security.pending_challenges')}</h3>
                <button class="btn btn-sm" onclick={() => load2FA()}>
                    <i class="fa-solid fa-rotate mr-1"></i>{t('common.refresh')}
                </button>
            </div>

            {#if challenges.length === 0}
                <div class="card p-6 text-center">
                    <i class="fa-solid fa-shield-check text-3xl text-green-400 mb-2"></i>
                    <p class="text-muted">{t('security.no_challenges')}</p>
                    <p class="text-xs text-muted mt-1">
                        When the agent attempts a dangerous operation (like executing shell commands),
                        a 2FA challenge will appear here for your confirmation.
                    </p>
                </div>
            {:else}
                {#each challenges as challenge}
                    <div class="card p-4">
                        <div class="flex items-start gap-3">
                            <i class="fa-solid fa-lock text-purple-400 text-lg mt-1"></i>
                            <div class="flex-1">
                                <div class="flex items-center gap-2">
                                    <span class="font-semibold text-heading">{challenge.tool}</span>
                                    <span class="text-xs bg-card-hover px-1.5 py-0.5 rounded">{challenge.source}</span>
                                    <span class="text-xs text-muted">{challenge.age_secs}s ago</span>
                                </div>
                                <p class="text-sm text-muted mt-1">{challenge.description}</p>
                                <div class="flex gap-2 mt-2">
                                    <button class="btn btn-sm bg-green-600 hover:bg-green-500 text-white" onclick={() => confirm2FA(challenge.id)}>
                                        <i class="fa-solid fa-check mr-1"></i>{t('common.confirm')}
                                    </button>
                                    <button class="btn btn-sm bg-red-600 hover:bg-red-500 text-white" onclick={() => reject2FA(challenge.id)}>
                                        <i class="fa-solid fa-xmark mr-1"></i>{t('security.reject')}
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>
                {/each}
            {/if}
        </div>
    {/if}
</div>
