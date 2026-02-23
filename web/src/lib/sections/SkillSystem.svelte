<script lang="ts">
	import CodeBlock from '$lib/components/CodeBlock.svelte';

	const skillToml = `[skill]
name = "calendar-reminder"
description = "Checks Google Calendar and sends Telegram reminders"
entrypoint = "main.py"
schedule = "*/10 * * * *"
venv = "auto"

[credentials]
GOOGLE_TOKEN = { oauth = "google" }
TELEGRAM_BOT_TOKEN = { env = "TELEGRAM_BOT_TOKEN" }`;

	const languages = [
		{ icon: 'fa-brands fa-python', name: 'Python', detail: 'Auto venv, pip install', color: 'text-yellow-400' },
		{ icon: 'fa-brands fa-node-js', name: 'Node.js', detail: 'npm/pnpm auto-install', color: 'text-green-400' },
		{ icon: 'fa-solid fa-fire', name: 'Rhai', detail: 'Embedded, zero-overhead', color: 'text-orange-400' },
		{ icon: 'fa-solid fa-terminal', name: 'Shell', detail: 'Classic scripts', color: 'text-slate-400' }
	];
</script>

<section id="skills" class="scroll-mt-20 py-20 md:py-28">
	<div class="mx-auto max-w-5xl px-6">
		<div class="mb-14 text-center">
			<h2 class="mb-4 text-3xl font-bold text-white md:text-4xl">Skills that grow with you</h2>
			<p class="mx-auto max-w-2xl text-slate-400">
				The agent can create, deploy, and manage its own skills. Drop a
				<code class="rounded bg-surface-raised px-1.5 py-0.5 font-mono text-sm text-accent-light">skill.toml</code>
				in a directory and it just works — credentials injected, lifecycle managed, logs captured.
			</p>
		</div>

		<div class="grid gap-8 lg:grid-cols-2">
			<!-- skill.toml example -->
			<div>
				<p class="mb-3 text-sm font-medium text-slate-500">
					<i class="fa-solid fa-file-code mr-1.5"></i>
					skill.toml
				</p>
				<CodeBlock code={skillToml} lang="toml" title="skills/calendar-reminder/skill.toml" />
			</div>

			<!-- Language support -->
			<div class="flex flex-col justify-center">
				<h3 class="mb-6 text-lg font-semibold text-white">Write skills in any language</h3>
				<div class="space-y-4">
					{#each languages as lang}
						<div class="flex items-center gap-4 rounded-lg border border-slate-800 bg-surface-raised px-5 py-4 transition-colors hover:border-slate-700">
							<i class="{lang.icon} {lang.color} text-xl w-6 text-center"></i>
							<div>
								<span class="font-medium text-white">{lang.name}</span>
								<span class="ml-2 text-sm text-slate-500">{lang.detail}</span>
							</div>
						</div>
					{/each}
				</div>

				<div class="mt-6 rounded-lg border border-slate-800 bg-surface-raised p-4">
					<p class="text-sm text-slate-400">
						<i class="fa-solid fa-wand-magic-sparkles mr-1.5 text-accent"></i>
						<strong class="text-slate-300">Auto-reconciliation</strong> — the skill manager watches your
						skills directory. Add, remove, or update a skill and it picks up changes automatically.
					</p>
				</div>
			</div>
		</div>
	</div>
</section>
