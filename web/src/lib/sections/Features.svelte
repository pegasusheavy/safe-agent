<script lang="ts">
	const features = [
		{
			icon: 'fa-solid fa-gavel',
			title: 'Human-Gated Execution',
			desc: 'The agent proposes — you decide. Every tool call goes through an approval queue before anything runs.'
		},
		{
			icon: 'fa-solid fa-puzzle-piece',
			title: 'Skill System',
			desc: 'Python, Node.js, Rhai, and shell skills with venvs, credential injection, and auto-reconciliation.'
		},
		{
			icon: 'fa-solid fa-brain',
			title: '6 LLM Backends',
			desc: 'Claude, Codex, Gemini, Aider, OpenRouter, or run a local GGUF model. Switch with one env var.'
		},
		{
			icon: 'fa-solid fa-diagram-project',
			title: 'Knowledge Graph',
			desc: 'SQLite-backed graph with typed nodes, edges, full-text search, and recursive traversal. It grows itself.'
		},
		{
			icon: 'fa-solid fa-shield-halved',
			title: 'Security Stack',
			desc: 'Landlock sandboxing, path traversal prevention, JWT auth, 2FA, passkeys, and PII encryption at rest.'
		},
		{
			icon: 'fa-solid fa-users',
			title: 'Multi-User',
			desc: 'Multiple user accounts with role-based access, timezone awareness, and per-user locale settings.'
		},
		{
			icon: 'fa-brands fa-telegram',
			title: 'Telegram & Dashboard',
			desc: 'Full bidirectional control from a Svelte web dashboard or Telegram. Approve actions from your phone.'
		},
		{
			icon: 'fa-solid fa-gauge-high',
			title: 'Live Dashboard',
			desc: 'Real-time activity feed, chat, memory browser, knowledge explorer, skill management, and settings.'
		},
		{
			icon: 'fa-solid fa-server',
			title: 'Self-Hosted',
			desc: 'Runs on your hardware. Docker image, ARM64 support. Your data never leaves your infrastructure.'
		}
	];
</script>

<section id="features" class="scroll-mt-20 py-20 md:py-28">
	<div class="mx-auto max-w-6xl px-6">
		<div class="mb-14 text-center">
			<h2 class="mb-4 text-3xl font-bold text-white md:text-4xl">Everything you need, nothing you don't</h2>
			<p class="mx-auto max-w-2xl text-slate-400">
				Built for people who want an AI assistant that's actually useful — without
				handing their data to someone else's cloud.
			</p>
		</div>

		<div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
			{#each features as feature}
				<div
					class="group rounded-xl border border-slate-800 bg-surface-raised p-6 transition-all hover:border-accent/30 hover:bg-surface-overlay/50"
				>
					<div
						class="mb-4 flex h-10 w-10 items-center justify-center rounded-lg bg-accent/10 text-accent transition-colors group-hover:bg-accent/20"
					>
						<i class="{feature.icon} text-lg"></i>
					</div>
					<h3 class="mb-2 font-semibold text-white">{feature.title}</h3>
					<p class="text-sm leading-relaxed text-slate-400">{feature.desc}</p>
				</div>
			{/each}
		</div>
	</div>
</section>
