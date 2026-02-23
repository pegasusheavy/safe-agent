<script lang="ts">
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import { dashboard } from '../lib/state.svelte';
    import { formatDateTime } from '../lib/time';
    import type { ActionResponse } from '../lib/types';

    interface GoalTask {
        id: string;
        goal_id: string;
        title: string;
        description: string;
        status: 'pending' | 'in_progress' | 'completed' | 'failed' | 'skipped';
        tool_call?: Record<string, unknown>;
        depends_on: string[];
        result?: string;
        sort_order: number;
        created_at: string;
        completed_at?: string;
    }

    interface Goal {
        id: string;
        title: string;
        description: string;
        status: 'active' | 'paused' | 'completed' | 'failed' | 'cancelled';
        priority: number;
        parent_goal_id?: string;
        reflection?: string;
        created_at: string;
        updated_at: string;
        completed_at?: string;
        total_tasks: number;
        completed_tasks: number;
        failed_tasks: number;
    }

    interface GoalDetail {
        goal: Goal;
        tasks: GoalTask[];
    }

    let goals = $state<Goal[]>([]);
    let error = $state(false);
    let statusFilter = $state<string>('');
    let expandedGoal = $state<string | null>(null);
    let goalDetail = $state<GoalDetail | null>(null);
    let detailLoading = $state(false);

    async function load() {
        error = false;
        try {
            const params = statusFilter ? `?status=${statusFilter}` : '';
            goals = await api<Goal[]>('GET', `/api/goals${params}`);
        } catch (e) {
            error = true;
            console.error('loadGoals:', e);
        }
    }

    async function loadDetail(goalId: string) {
        detailLoading = true;
        try {
            goalDetail = await api<GoalDetail>('GET', `/api/goals/${encodeURIComponent(goalId)}`);
        } catch (e) {
            console.error('loadGoalDetail:', e);
        } finally {
            detailLoading = false;
        }
    }

    async function updateStatus(goalId: string, newStatus: string) {
        try {
            await api<ActionResponse>('PUT', `/api/goals/${encodeURIComponent(goalId)}/status`, {
                status: newStatus,
            });
            load();
            if (expandedGoal === goalId) {
                loadDetail(goalId);
            }
        } catch (e) {
            alert(t('goals.status_update_failed') + (e as Error).message);
        }
    }

    function toggleExpand(goalId: string) {
        if (expandedGoal === goalId) {
            expandedGoal = null;
            goalDetail = null;
        } else {
            expandedGoal = goalId;
            loadDetail(goalId);
        }
    }

    function statusColor(status: string): string {
        switch (status) {
            case 'active': return 'bg-primary-500/15 text-primary-400';
            case 'paused': return 'bg-warning-500/15 text-warning-500';
            case 'completed': return 'bg-success-500/15 text-success-500';
            case 'failed': return 'bg-error-500/12 text-error-400';
            case 'cancelled': return 'bg-text-subtle/10 text-text-subtle';
            case 'in_progress': return 'bg-primary-500/15 text-primary-400';
            case 'pending': return 'bg-text-subtle/10 text-text-muted';
            case 'skipped': return 'bg-text-subtle/10 text-text-subtle';
            default: return 'bg-text-subtle/10 text-text-subtle';
        }
    }

    function statusIcon(status: string): string {
        switch (status) {
            case 'active': return 'fa-circle-play';
            case 'paused': return 'fa-circle-pause';
            case 'completed': return 'fa-circle-check';
            case 'failed': return 'fa-circle-xmark';
            case 'cancelled': return 'fa-ban';
            case 'in_progress': return 'fa-spinner fa-spin';
            case 'pending': return 'fa-clock';
            case 'skipped': return 'fa-forward';
            default: return 'fa-question';
        }
    }

    $effect(() => {
        if (dashboard.currentTab === 'goals') {
            dashboard.refreshCounter;
            load();
        }
    });
</script>

<section class="bg-surface border border-border rounded-lg shadow-sm overflow-hidden">
    <div class="flex justify-between items-center border-b border-border">
        <h2 class="text-xs font-semibold px-4 py-3 uppercase tracking-wider text-text-muted">
            <i class="fa-solid fa-bullseye mr-1.5"></i> {t('goals.title')}
        </h2>
        <div class="flex items-center gap-2 pr-3">
            <select
                bind:value={statusFilter}
                onchange={() => load()}
                class="text-xs px-2 py-1 border border-border rounded-md bg-surface text-text outline-none focus:border-primary-500"
            >
                <option value="">{t('goals.all_statuses')}</option>
                <option value="active">{t('goals.active')}</option>
                <option value="paused">{t('goals.paused')}</option>
                <option value="completed">{t('goals.completed')}</option>
                <option value="failed">{t('goals.failed')}</option>
                <option value="cancelled">{t('goals.cancelled')}</option>
            </select>
            <span class="text-xs text-text-muted">
                {t('goals.count', { count: goals.length })}
            </span>
            <button
                onclick={load}
                class="px-2.5 py-1 text-xs border border-border rounded-md bg-surface hover:bg-surface-elevated transition-colors"
            >
                <i class="fa-solid fa-arrows-rotate mr-1"></i> {t('common.refresh')}
            </button>
        </div>
    </div>

    <div class="p-3">
        {#if error}
            <p class="text-text-subtle text-sm italic text-center py-4">{t('goals.error_loading')}</p>
        {:else if goals.length === 0}
            <p class="text-text-subtle text-sm italic text-center py-4">
                {t('goals.no_goals')}
            </p>
        {:else}
            {#each goals as goal (goal.id)}
                <div class="border border-border rounded-md mb-3 bg-surface-muted overflow-hidden">
                    <!-- Goal header -->
                    <button
                        onclick={() => toggleExpand(goal.id)}
                        class="w-full p-4 flex justify-between items-center hover:bg-surface-elevated/40 transition-colors cursor-pointer text-left"
                    >
                        <div class="flex items-center gap-2 flex-1 min-w-0">
                            <i class="fa-solid fa-chevron-right text-[10px] text-text-subtle transition-transform {expandedGoal === goal.id ? 'rotate-90' : ''}"></i>
                            <span class="text-[15px] font-semibold truncate">{goal.title}</span>
                            {#if goal.priority > 0}
                                <span class="text-[11px] px-1.5 py-0.5 rounded-full bg-warning-500/15 text-warning-500 font-medium shrink-0">
                                    P{goal.priority}
                                </span>
                            {/if}
                        </div>
                        <div class="flex items-center gap-2 shrink-0 ml-3">
                            {#if goal.total_tasks > 0}
                                <div class="flex items-center gap-1.5">
                                    <div class="w-20 h-1.5 bg-border rounded-full overflow-hidden">
                                        <div
                                            class="h-full bg-success-500 rounded-full transition-all"
                                            style="width: {goal.total_tasks > 0 ? (goal.completed_tasks / goal.total_tasks) * 100 : 0}%"
                                        ></div>
                                    </div>
                                    <span class="text-[11px] text-text-muted font-mono">
                                        {goal.completed_tasks}/{goal.total_tasks}
                                    </span>
                                </div>
                            {/if}
                            <span class="text-[11px] px-2 py-0.5 rounded-full font-medium {statusColor(goal.status)}">
                                <i class="fa-solid {statusIcon(goal.status)} mr-1"></i>{goal.status}
                            </span>
                        </div>
                    </button>

                    {#if goal.description && expandedGoal !== goal.id}
                        <div class="text-sm text-text-muted px-4 pb-3 -mt-1">{goal.description}</div>
                    {/if}

                    <!-- Expanded detail -->
                    {#if expandedGoal === goal.id}
                        <div class="border-t border-border">
                            {#if goal.description}
                                <div class="text-sm text-text-muted px-4 pt-3 pb-2">{goal.description}</div>
                            {/if}

                            <!-- Action buttons -->
                            <div class="flex items-center gap-2 px-4 py-2 border-b border-border">
                                {#if goal.status === 'active'}
                                    <button
                                        onclick={() => updateStatus(goal.id, 'paused')}
                                        class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface text-warning-500 hover:bg-warning-500/10 hover:border-warning-500 transition-colors"
                                    >
                                        <i class="fa-solid fa-pause mr-1"></i>{t('goals.pause')}
                                    </button>
                                    <button
                                        onclick={() => updateStatus(goal.id, 'cancelled')}
                                        class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface text-error-400 hover:bg-error-500/10 hover:border-error-500 transition-colors"
                                    >
                                        <i class="fa-solid fa-ban mr-1"></i>{t('common.cancel')}
                                    </button>
                                {:else if goal.status === 'paused'}
                                    <button
                                        onclick={() => updateStatus(goal.id, 'active')}
                                        class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface text-success-500 hover:bg-success-500/10 hover:border-success-500 transition-colors"
                                    >
                                        <i class="fa-solid fa-play mr-1"></i>{t('goals.resume')}
                                    </button>
                                    <button
                                        onclick={() => updateStatus(goal.id, 'cancelled')}
                                        class="px-3 py-1.5 text-xs border border-border rounded-md bg-surface text-error-400 hover:bg-error-500/10 hover:border-error-500 transition-colors"
                                    >
                                        <i class="fa-solid fa-ban mr-1"></i>{t('common.cancel')}
                                    </button>
                                {/if}
                            </div>

                            {#if detailLoading}
                                <div class="p-4 text-sm text-text-subtle italic text-center">
                                    <i class="fa-solid fa-spinner fa-spin mr-1"></i>{t('common.loading')}
                                </div>
                            {:else if goalDetail}
                                <!-- Reflection -->
                                {#if goalDetail.goal.reflection}
                                    <div class="mx-4 mt-3 p-3 bg-primary-950/40 border border-primary-800/30 rounded-md">
                                        <div class="text-[11px] font-semibold uppercase tracking-wider text-primary-400 mb-1">
                                            <i class="fa-solid fa-brain mr-1"></i> {t('goals.self_reflection')}
                                        </div>
                                        <div class="text-xs text-text-muted leading-relaxed">
                                            {goalDetail.goal.reflection}
                                        </div>
                                    </div>
                                {/if}

                                <!-- Tasks list -->
                                <div class="p-4">
                                    <div class="text-xs font-semibold uppercase tracking-wider text-text-muted mb-3">
                                        <i class="fa-solid fa-list-check mr-1"></i>
                                        {t('goals.tasks')} ({goalDetail.tasks.length})
                                    </div>

                                    {#if goalDetail.tasks.length === 0}
                                        <p class="text-text-subtle text-sm italic text-center py-2">
                                            {t('goals.no_tasks')}
                                        </p>
                                    {:else}
                                        <div class="space-y-2">
                                            {#each goalDetail.tasks as task, i (task.id)}
                                                <div class="flex items-start gap-3 p-3 bg-surface border border-border rounded-md">
                                                    <div class="shrink-0 mt-0.5">
                                                        <span class="inline-flex items-center justify-center w-6 h-6 rounded-full text-[11px] font-semibold {statusColor(task.status)}">
                                                            <i class="fa-solid {statusIcon(task.status)}"></i>
                                                        </span>
                                                    </div>
                                                    <div class="flex-1 min-w-0">
                                                        <div class="flex items-center gap-2 mb-0.5">
                                                            <span class="text-sm font-medium">{task.title}</span>
                                                            <span class="text-[10px] px-1.5 py-0.5 rounded-full {statusColor(task.status)}">
                                                                {task.status}
                                                            </span>
                                                        </div>
                                                        {#if task.description}
                                                            <div class="text-xs text-text-muted mb-1">{task.description}</div>
                                                        {/if}
                                                        {#if task.depends_on.length > 0}
                                                            <div class="text-[10px] text-text-subtle">
                                                                {t('goals.depends_on')} {task.depends_on.join(', ')}
                                                            </div>
                                                        {/if}
                                                        {#if task.result}
                                                            <div class="mt-1.5 p-2 bg-background border border-border rounded text-[11px] font-mono text-text-muted max-h-[120px] overflow-auto whitespace-pre-wrap break-all leading-relaxed">
                                                                {task.result}
                                                            </div>
                                                        {/if}
                                                        {#if task.tool_call}
                                                            <div class="mt-1 text-[10px] text-accent-300 font-mono">
                                                                {t('goals.tool')} {JSON.stringify(task.tool_call)}
                                                            </div>
                                                        {/if}
                                                    </div>
                                                </div>
                                            {/each}
                                        </div>
                                    {/if}
                                </div>

                                <!-- Metadata -->
                                <div class="px-4 pb-4">
                                    <div class="grid grid-cols-2 gap-x-4 gap-y-1 text-[11px]">
                                        <span class="text-text-subtle">{t('goals.created')}</span>
                                        <span class="text-text-muted font-mono">{formatDateTime(goalDetail.goal.created_at)}</span>
                                        <span class="text-text-subtle">{t('goals.updated')}</span>
                                        <span class="text-text-muted font-mono">{formatDateTime(goalDetail.goal.updated_at)}</span>
                                        {#if goalDetail.goal.completed_at}
                                            <span class="text-text-subtle">{t('goals.completed_at')}</span>
                                            <span class="text-text-muted font-mono">{formatDateTime(goalDetail.goal.completed_at)}</span>
                                        {/if}
                                        <span class="text-text-subtle">{t('goals.id')}</span>
                                        <span class="text-text-muted font-mono truncate" title={goalDetail.goal.id}>{goalDetail.goal.id}</span>
                                    </div>
                                </div>
                            {/if}
                        </div>
                    {/if}
                </div>
            {/each}
        {/if}
    </div>
</section>
