<script lang="ts">
    import { auth } from '../lib/state.svelte';
    import { t } from '../lib/i18n';

    let currentStep = $state(1);
    const totalSteps = 4;

    // Step 1: Agent Identity
    let agentName = $state('');
    let corePersonality = $state('');

    // Step 2: LLM Backend
    let activeBackend = $state('');
    let availableBackends = $state<string[]>([]);
    let llmTestResult = $state('');
    let llmTestOk = $state<boolean | null>(null);
    let llmTesting = $state(false);

    // Ollama sub-state for Step 2
    let ollamaSpecs: any = $state(null);
    let ollamaRecs: any[] = $state([]);
    let ollamaRecsLoading = $state(false);
    let ollamaAvail: any = $state(null);
    let ollamaPulling = $state('');
    let ollamaPullMsg = $state('');

    // Step 3: Messaging
    let telegramEnabled = $state(false);
    let whatsappEnabled = $state(false);

    // General
    let saving = $state(false);
    let error = $state('');
    let completing = $state(false);

    // Dispatch "complete" event so parent knows to switch views
    const dispatch = (name: string) => {
        window.dispatchEvent(new CustomEvent(name));
    };

    async function loadOllamaAdvisor() {
        ollamaRecsLoading = true;
        try {
            const [specRes, recRes, statusRes] = await Promise.all([
                fetch('/api/llm/advisor/system'),
                fetch('/api/llm/advisor/recommend?limit=3'),
                fetch('/api/llm/ollama/status'),
            ]);
            ollamaSpecs = await specRes.json();
            const recData = await recRes.json();
            ollamaRecs = recData.models || [];
            ollamaAvail = await statusRes.json();
        } catch (e) {
            console.error('Failed to load Ollama advisor', e);
        }
        ollamaRecsLoading = false;
    }

    async function onboardingPull(tag: string) {
        ollamaPulling = tag;
        ollamaPullMsg = t('onboarding.ollama_pulling');
        try {
            const res = await fetch('/api/llm/ollama/pull', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ tag }),
            });
            const data = await res.json();
            if (data.ok) {
                ollamaPullMsg = t('onboarding.ollama_installed');
                const statusRes = await fetch('/api/llm/ollama/status');
                ollamaAvail = await statusRes.json();
                const recRes = await fetch('/api/llm/advisor/recommend?limit=3');
                const recData = await recRes.json();
                ollamaRecs = recData.models || [];
            } else {
                ollamaPullMsg = data.error || 'Pull failed';
            }
        } catch (e) {
            ollamaPullMsg = 'Pull failed';
        }
        setTimeout(() => { ollamaPulling = ''; ollamaPullMsg = ''; }, 3000);
    }

    async function onboardingUseModel(tag: string) {
        try {
            await fetch('/api/llm/ollama/configure', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ model: tag }),
            });
            llmTestOk = true;
            llmTestResult = t('onboarding.ollama_configured', { model: tag });
        } catch { /* ignore */ }
    }

    $effect(() => {
        loadOnboardingStatus();
    });

    $effect(() => {
        if (activeBackend === 'ollama' && !ollamaSpecs) {
            loadOllamaAdvisor();
        }
    });

    async function loadOnboardingStatus() {
        try {
            const res = await fetch('/api/onboarding/status');
            const data = await res.json();
            agentName = data.agent_name || '';
            corePersonality = data.core_personality || '';
            activeBackend = data.llm_backend || '';
            availableBackends = data.llm_available || [];
            telegramEnabled = data.telegram_enabled || false;
            whatsappEnabled = data.whatsapp_enabled || false;
        } catch (e) {
            console.error('Failed to load onboarding status', e);
        }
    }

    async function saveConfig() {
        saving = true;
        error = '';
        try {
            const res = await fetch('/api/onboarding/save-config', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    agent_name: agentName || undefined,
                    core_personality: corePersonality || undefined,
                    llm_backend: activeBackend || undefined,
                }),
            });
            const data = await res.json();
            if (!data.ok) {
                error = data.message || t('onboarding.failed_save');
            }
        } catch (e) {
            error = t('onboarding.network_error_save');
        } finally {
            saving = false;
        }
    }

    async function testLlm() {
        llmTesting = true;
        llmTestResult = '';
        llmTestOk = null;
        try {
            const res = await fetch('/api/onboarding/test-llm', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
            });
            const data = await res.json();
            llmTestOk = data.ok;
            llmTestResult = data.ok ? data.response : data.error;
        } catch (e) {
            llmTestOk = false;
            llmTestResult = t('onboarding.network_error_test');
        } finally {
            llmTesting = false;
        }
    }

    async function completeOnboarding() {
        completing = true;
        error = '';
        try {
            const res = await fetch('/api/onboarding/complete', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    agent_name: agentName || undefined,
                    core_personality: corePersonality || undefined,
                }),
            });
            const data = await res.json();
            if (data.ok) {
                dispatch('onboarding-complete');
            } else {
                error = data.message || t('onboarding.failed_complete');
            }
        } catch (e) {
            error = t('onboarding.network_error_complete');
        } finally {
            completing = false;
        }
    }

    function nextStep() {
        if (currentStep === 1) {
            saveConfig();
        }
        if (currentStep < totalSteps) {
            currentStep++;
        }
    }

    function prevStep() {
        if (currentStep > 1) {
            currentStep--;
        }
    }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-bg/95 backdrop-blur-sm">
    <div class="w-full max-w-2xl mx-4">
        <!-- Step indicator -->
        <div class="flex items-center justify-center mb-8 gap-2">
            {#each Array(totalSteps) as _, i}
                {@const step = i + 1}
                <div class="flex items-center">
                    <div class="w-8 h-8 rounded-full flex items-center justify-center text-xs font-bold transition-colors
                        {step < currentStep ? 'bg-green-600 text-white' : step === currentStep ? 'bg-primary-600 text-white' : 'bg-surface-elevated text-text-muted border border-border'}">
                        {#if step < currentStep}
                            <i class="fa-solid fa-check"></i>
                        {:else}
                            {step}
                        {/if}
                    </div>
                    {#if i < totalSteps - 1}
                        <div class="w-12 h-0.5 mx-1 {step < currentStep ? 'bg-green-600' : 'bg-border'}"></div>
                    {/if}
                </div>
            {/each}
        </div>

        <div class="bg-surface rounded-xl border border-border shadow-2xl overflow-hidden">
            <!-- Step content -->
            <div class="p-8">
                {#if currentStep === 1}
                    <!-- Step 1: Welcome / Agent Identity -->
                    <div class="text-center mb-6">
                        <div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-primary-600/10 mb-4">
                            <i class="fa-solid fa-robot text-3xl text-primary-500"></i>
                        </div>
                        <h2 class="text-2xl font-bold text-text">{t('onboarding.welcome')}</h2>
                        <p class="text-text-muted mt-2">{t('onboarding.welcome_desc')}</p>
                    </div>

                    <label class="block mb-4">
                        <span class="text-xs font-medium text-text-muted uppercase tracking-wide">{t('onboarding.agent_name')}</span>
                        <input
                            type="text"
                            bind:value={agentName}
                            class="mt-1 w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text
                                   placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50"
                            placeholder={t('onboarding.agent_name_placeholder')}
                        />
                    </label>

                    <label class="block mb-2">
                        <span class="text-xs font-medium text-text-muted uppercase tracking-wide">{t('onboarding.core_personality')}</span>
                        <textarea
                            bind:value={corePersonality}
                            rows="4"
                            class="mt-1 w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text
                                   placeholder-text-muted/50 focus:outline-none focus:ring-2 focus:ring-primary-500/50 resize-none"
                            placeholder={t('onboarding.personality_placeholder')}
                        ></textarea>
                    </label>
                    <p class="text-xs text-text-muted">{t('onboarding.personality_hint')}</p>

                {:else if currentStep === 2}
                    <!-- Step 2: LLM Backend -->
                    <div class="text-center mb-6">
                        <div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-primary-600/10 mb-4">
                            <i class="fa-solid fa-brain text-3xl text-primary-500"></i>
                        </div>
                        <h2 class="text-2xl font-bold text-text">{t('onboarding.llm_backend')}</h2>
                        <p class="text-text-muted mt-2">{t('onboarding.llm_desc')}</p>
                    </div>

                    <label class="block mb-4">
                        <span class="text-xs font-medium text-text-muted uppercase tracking-wide">{t('onboarding.backend_label')}</span>
                        <select
                            bind:value={activeBackend}
                            class="mt-1 w-full px-3 py-2 rounded-md border border-border bg-surface-elevated text-text
                                   focus:outline-none focus:ring-2 focus:ring-primary-500/50"
                        >
                            {#each availableBackends as backend}
                                <option value={backend}>{backend}</option>
                            {/each}
                        </select>
                    </label>

                    <p class="text-xs text-text-muted mb-4">
                        {t('onboarding.env_hint')}
                        (e.g. <code class="bg-surface-elevated px-1 rounded text-xs">OPENROUTER_API_KEY</code>,
                        <code class="bg-surface-elevated px-1 rounded text-xs">CLAUDE_BIN</code>, etc.)
                    </p>

                    <!-- Ollama Advisor inline -->
                    {#if activeBackend === 'ollama'}
                        <div class="mb-4 p-4 rounded-lg border border-border bg-surface-elevated space-y-3">
                            {#if ollamaRecsLoading}
                                <p class="text-sm text-text-muted"><i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('onboarding.ollama_detecting')}</p>
                            {:else if ollamaSpecs}
                                <div class="text-sm">
                                    <span class="font-medium">{t('onboarding.ollama_your_hardware')}</span>
                                    <span class="text-text-muted ml-2">
                                        {ollamaSpecs.total_ram_gb?.toFixed(0)} GB RAM
                                        {#if ollamaSpecs.has_gpu}
                                            · {ollamaSpecs.gpu_name || 'GPU'} ({ollamaSpecs.gpu_vram_gb?.toFixed(0) ?? '?'} GB VRAM) · {ollamaSpecs.backend}
                                        {:else}
                                            · CPU only
                                        {/if}
                                    </span>
                                </div>

                                {#if ollamaAvail}
                                    <div class="flex items-center gap-2 text-sm">
                                        <span class="w-2 h-2 rounded-full {ollamaAvail.available ? 'bg-green-500' : 'bg-red-500'}"></span>
                                        <span>{ollamaAvail.available ? t('onboarding.ollama_connected') : t('onboarding.ollama_not_running')}</span>
                                    </div>
                                {/if}

                                {#if ollamaRecs.length}
                                    <div class="text-xs font-medium text-text-muted uppercase tracking-wide">{t('onboarding.ollama_top_models')}</div>
                                    <div class="space-y-2">
                                        {#each ollamaRecs as rec}
                                            <div class="flex items-center justify-between p-2 rounded bg-surface border border-border/50">
                                                <div>
                                                    <span class="font-mono text-sm text-text">{rec.name}</span>
                                                    <span class="text-xs text-text-muted ml-2">{rec.params_b?.toFixed(1)}B · {rec.best_quant} · {t('onboarding.ollama_score')}: {rec.score?.toFixed(0)}</span>
                                                </div>
                                                <div>
                                                    {#if rec.installed}
                                                        <button
                                                            class="bg-green-600/20 text-green-400 px-2 py-1 rounded text-xs hover:bg-green-600/30"
                                                            onclick={() => onboardingUseModel(rec.ollama_tag || rec.name)}
                                                        >
                                                            {t('onboarding.ollama_use')}
                                                        </button>
                                                    {:else if rec.ollama_tag && ollamaAvail?.available}
                                                        {#if ollamaPulling === rec.ollama_tag}
                                                            <span class="text-xs text-text-muted"><i class="fa-solid fa-spinner fa-spin mr-1"></i>{ollamaPullMsg}</span>
                                                        {:else}
                                                            <button
                                                                class="bg-surface text-text px-2 py-1 rounded text-xs hover:bg-surface-elevated border border-border"
                                                                onclick={() => onboardingPull(rec.ollama_tag)}
                                                            >
                                                                <i class="fa-solid fa-download mr-1"></i>{t('onboarding.ollama_install')}
                                                            </button>
                                                        {/if}
                                                    {:else}
                                                        <span class="text-xs text-text-muted">—</span>
                                                    {/if}
                                                </div>
                                            </div>
                                        {/each}
                                    </div>
                                {/if}

                                {#if !ollamaAvail?.available}
                                    <p class="text-xs text-text-muted">
                                        {t('onboarding.ollama_install_hint')}
                                    </p>
                                {/if}
                            {/if}
                        </div>
                    {/if}

                    <button
                        onclick={testLlm}
                        disabled={llmTesting}
                        class="w-full px-4 py-2 rounded-md border border-border bg-surface-elevated text-text font-medium text-sm
                               hover:bg-surface transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                        {#if llmTesting}
                            <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('onboarding.testing')}
                        {:else}
                            <i class="fa-solid fa-flask mr-1"></i> {t('onboarding.test_connection')}
                        {/if}
                    </button>

                    {#if llmTestOk !== null}
                        <div class="mt-4 p-3 rounded-md text-sm {llmTestOk ? 'bg-green-900/30 border border-green-700/50 text-green-300' : 'bg-red-900/30 border border-red-700/50 text-red-300'}">
                            {#if llmTestOk}
                                <i class="fa-solid fa-check-circle mr-1"></i> <strong>{t('onboarding.test_success')}</strong>
                                <p class="mt-1 text-xs opacity-80">{llmTestResult}</p>
                            {:else}
                                <i class="fa-solid fa-times-circle mr-1"></i> <strong>{t('onboarding.test_failed')}</strong>
                                <p class="mt-1 text-xs opacity-80">{llmTestResult}</p>
                            {/if}
                        </div>
                    {/if}

                {:else if currentStep === 3}
                    <!-- Step 3: Messaging (optional) -->
                    <div class="text-center mb-6">
                        <div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-primary-600/10 mb-4">
                            <i class="fa-solid fa-comments text-3xl text-primary-500"></i>
                        </div>
                        <h2 class="text-2xl font-bold text-text">{t('onboarding.messaging')}</h2>
                        <p class="text-text-muted mt-2">{t('onboarding.messaging_desc')}</p>
                    </div>

                    <div class="space-y-4">
                        <div class="flex items-center justify-between p-4 rounded-lg border border-border bg-surface-elevated">
                            <div class="flex items-center gap-3">
                                <i class="fa-brands fa-telegram text-2xl text-blue-400"></i>
                                <div>
                                    <p class="font-medium text-text">{t('onboarding.telegram')}</p>
                                    <p class="text-xs text-text-muted">{t('onboarding.telegram_requires')}</p>
                                </div>
                            </div>
                            <span class="px-2 py-1 text-xs rounded-full font-medium
                                {telegramEnabled ? 'bg-green-900/40 text-green-400 border border-green-700/50' : 'bg-surface text-text-muted border border-border'}">
                                {telegramEnabled ? t('common.enabled') : t('common.not_configured')}
                            </span>
                        </div>

                        <div class="flex items-center justify-between p-4 rounded-lg border border-border bg-surface-elevated">
                            <div class="flex items-center gap-3">
                                <i class="fa-brands fa-whatsapp text-2xl text-green-400"></i>
                                <div>
                                    <p class="font-medium text-text">{t('onboarding.whatsapp')}</p>
                                    <p class="text-xs text-text-muted">{t('onboarding.whatsapp_requires')}</p>
                                </div>
                            </div>
                            <span class="px-2 py-1 text-xs rounded-full font-medium
                                {whatsappEnabled ? 'bg-green-900/40 text-green-400 border border-green-700/50' : 'bg-surface text-text-muted border border-border'}">
                                {whatsappEnabled ? t('common.enabled') : t('common.not_configured')}
                            </span>
                        </div>
                    </div>

                    <p class="text-xs text-text-muted mt-4">
                        {t('onboarding.messaging_hint')}
                    </p>

                {:else if currentStep === 4}
                    <!-- Step 4: Review / Finish -->
                    <div class="text-center mb-6">
                        <div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-green-600/10 mb-4">
                            <i class="fa-solid fa-check-double text-3xl text-green-500"></i>
                        </div>
                        <h2 class="text-2xl font-bold text-text">{t('onboarding.ready')}</h2>
                        <p class="text-text-muted mt-2">{t('onboarding.ready_desc')}</p>
                    </div>

                    <div class="space-y-3 mb-6">
                        <div class="flex justify-between items-center p-3 rounded-lg bg-surface-elevated border border-border">
                            <span class="text-sm text-text-muted">{t('onboarding.review_name')}</span>
                            <span class="text-sm font-medium text-text">{agentName || t('onboarding.default')}</span>
                        </div>
                        <div class="flex justify-between items-center p-3 rounded-lg bg-surface-elevated border border-border">
                            <span class="text-sm text-text-muted">{t('onboarding.review_backend')}</span>
                            <span class="text-sm font-medium text-text">{activeBackend || t('onboarding.default')}</span>
                        </div>
                        <div class="flex justify-between items-center p-3 rounded-lg bg-surface-elevated border border-border">
                            <span class="text-sm text-text-muted">{t('onboarding.review_telegram')}</span>
                            <span class="text-sm font-medium {telegramEnabled ? 'text-green-400' : 'text-text-muted'}">
                                {telegramEnabled ? t('common.enabled') : t('common.not_configured')}
                            </span>
                        </div>
                        <div class="flex justify-between items-center p-3 rounded-lg bg-surface-elevated border border-border">
                            <span class="text-sm text-text-muted">{t('onboarding.review_whatsapp')}</span>
                            <span class="text-sm font-medium {whatsappEnabled ? 'text-green-400' : 'text-text-muted'}">
                                {whatsappEnabled ? t('common.enabled') : t('common.not_configured')}
                            </span>
                        </div>
                        {#if corePersonality}
                            <div class="p-3 rounded-lg bg-surface-elevated border border-border">
                                <span class="text-xs text-text-muted uppercase tracking-wide">{t('onboarding.review_personality')}</span>
                                <p class="text-sm text-text mt-1 line-clamp-3">{corePersonality}</p>
                            </div>
                        {/if}
                    </div>

                    <p class="text-xs text-text-muted text-center mb-2">
                        {t('onboarding.change_later')}
                    </p>
                {/if}
            </div>

            <!-- Error message -->
            {#if error}
                <div class="px-8 pb-2">
                    <div class="p-3 rounded-md bg-red-900/30 border border-red-700/50 text-red-300 text-sm">
                        <i class="fa-solid fa-triangle-exclamation mr-1"></i> {error}
                    </div>
                </div>
            {/if}

            <!-- Navigation buttons -->
            <div class="flex justify-between items-center px-8 py-5 bg-surface-elevated border-t border-border">
                <div>
                    {#if currentStep > 1}
                        <button
                            onclick={prevStep}
                            class="px-4 py-2 rounded-md text-sm text-text-muted hover:text-text transition-colors"
                        >
                            <i class="fa-solid fa-arrow-left mr-1"></i> {t('common.back')}
                        </button>
                    {/if}
                </div>

                <div class="flex items-center gap-2">
                    <span class="text-xs text-text-muted">{t('onboarding.step_of', { current: currentStep, total: totalSteps })}</span>

                    {#if currentStep === 3}
                        <button
                            onclick={nextStep}
                            class="px-4 py-2 rounded-md text-sm text-text-muted hover:text-text transition-colors"
                        >
                            {t('common.skip')} <i class="fa-solid fa-forward ml-1"></i>
                        </button>
                    {/if}

                    {#if currentStep < totalSteps}
                        <button
                            onclick={nextStep}
                            disabled={saving}
                            class="px-5 py-2 rounded-md bg-primary-600 text-white font-medium text-sm
                                   hover:bg-primary-500 transition-colors disabled:opacity-50"
                        >
                            {#if saving}
                                <i class="fa-solid fa-spinner fa-spin mr-1"></i> Saving...
                            {:else}
                                Next <i class="fa-solid fa-arrow-right ml-1"></i>
                            {/if}
                        </button>
                    {:else}
                        <button
                            onclick={completeOnboarding}
                            disabled={completing}
                            class="px-5 py-2 rounded-md bg-green-600 text-white font-medium text-sm
                                   hover:bg-green-500 transition-colors disabled:opacity-50"
                        >
                            {#if completing}
                                <i class="fa-solid fa-spinner fa-spin mr-1"></i> Completing...
                            {:else}
                                <i class="fa-solid fa-rocket mr-1"></i> Complete Setup
                            {/if}
                        </button>
                    {/if}
                </div>
            </div>
        </div>
    </div>
</div>
