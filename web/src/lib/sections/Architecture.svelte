<script lang="ts">
	const layers = [
		{
			label: 'Interfaces',
			color: 'border-sky-500/40 bg-sky-500/5',
			iconColor: 'text-sky-400',
			items: [
				{ icon: 'fa-brands fa-telegram', name: 'Telegram Bot' },
				{ icon: 'fa-solid fa-gauge-high', name: 'Web Dashboard' },
				{ icon: 'fa-solid fa-code', name: 'REST API' }
			]
		},
		{
			label: 'Agent Core',
			color: 'border-accent/40 bg-accent/5',
			iconColor: 'text-accent',
			items: [
				{ icon: 'fa-solid fa-robot', name: 'Agent Loop' },
				{ icon: 'fa-solid fa-check-double', name: 'Approval Queue' },
				{ icon: 'fa-solid fa-brain', name: 'LLM Engine' }
			]
		},
		{
			label: 'Storage',
			color: 'border-violet-500/40 bg-violet-500/5',
			iconColor: 'text-violet-400',
			items: [
				{ icon: 'fa-solid fa-memory', name: 'Memory (Core / Archival)' },
				{ icon: 'fa-solid fa-diagram-project', name: 'Knowledge Graph' },
				{ icon: 'fa-solid fa-database', name: 'SQLite (WAL + FTS5)' }
			]
		},
		{
			label: 'Execution',
			color: 'border-amber-500/40 bg-amber-500/5',
			iconColor: 'text-amber-400',
			items: [
				{ icon: 'fa-solid fa-toolbox', name: '10 Built-in Tools' },
				{ icon: 'fa-solid fa-puzzle-piece', name: 'Skill Manager' },
				{ icon: 'fa-solid fa-key', name: 'OAuth (10 Providers)' }
			]
		}
	];

	const backends = [
		'Claude CLI',
		'Codex CLI',
		'Gemini CLI',
		'Aider',
		'OpenRouter',
		'Local GGUF'
	];
</script>

<section id="architecture" class="scroll-mt-20 py-20 md:py-28">
	<div class="mx-auto max-w-5xl px-6">
		<div class="mb-14 text-center">
			<h2 class="mb-4 text-3xl font-bold text-white md:text-4xl">How it all fits together</h2>
			<p class="mx-auto max-w-2xl text-slate-400">
				Messages come in from Telegram or the dashboard, flow through an approval queue,
				and the agent executes with the tools you've allowed.
			</p>
		</div>

		<!-- Architecture layers -->
		<div class="space-y-4">
			{#each layers as layer, i}
				<div class="rounded-xl border {layer.color} p-5">
					<div class="mb-3 flex items-center gap-2">
						<span class="text-xs font-semibold uppercase tracking-wider text-slate-500">{layer.label}</span>
						{#if i < layers.length - 1}
							<div class="flex-1 border-t border-slate-800"></div>
						{/if}
					</div>
					<div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
						{#each layer.items as item}
							<div class="flex items-center gap-3 rounded-lg bg-slate-900/50 px-4 py-3">
								<i class="{item.icon} {layer.iconColor}"></i>
								<span class="text-sm text-slate-300">{item.name}</span>
							</div>
						{/each}
					</div>
				</div>

				{#if i < layers.length - 1}
					<div class="flex justify-center">
						<i class="fa-solid fa-arrow-down text-slate-700"></i>
					</div>
				{/if}
			{/each}
		</div>

		<!-- LLM backends row -->
		<div class="mt-10 text-center">
			<p class="mb-4 text-xs font-semibold uppercase tracking-wider text-slate-500">LLM Backends</p>
			<div class="flex flex-wrap justify-center gap-3">
				{#each backends as b}
					<span class="rounded-full border border-slate-800 bg-surface-raised px-4 py-1.5 text-xs text-slate-400">
						{b}
					</span>
				{/each}
			</div>
		</div>
	</div>
</section>
