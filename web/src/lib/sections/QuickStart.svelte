<script lang="ts">
	import CodeBlock from '$lib/components/CodeBlock.svelte';

	const steps = [
		{
			num: '01',
			title: 'Create your directories',
			desc: 'One folder for data, one for config. That\'s it.',
			code: `mkdir -p ~/.local/share/safe-agent
mkdir -p ~/.config/safe-agent
curl -fsSL https://raw.githubusercontent.com/pegasusheavy/safe-agent/main/config.example.toml \\
  -o ~/.config/safe-agent/config.toml
curl -fsSL https://raw.githubusercontent.com/pegasusheavy/safe-agent/main/.env.example \\
  -o ~/.config/safe-agent/.env`
		},
		{
			num: '02',
			title: 'Set your secrets',
			desc: 'Edit the .env file with your dashboard password and a JWT secret.',
			code: `# Edit ~/.config/safe-agent/.env
DASHBOARD_PASSWORD=pick-a-strong-password
JWT_SECRET=$(openssl rand -hex 32)`
		},
		{
			num: '03',
			title: 'Run the container',
			desc: 'One command. Dashboard is at localhost:3031.',
			code: `docker run -d \\
  --name safe-agent \\
  --restart unless-stopped \\
  -p 3031:3031 \\
  -v ~/.local/share/safe-agent:/data/safe-agent \\
  -v ~/.config/safe-agent/config.toml:/config/safe-agent/config.toml:ro \\
  --env-file ~/.config/safe-agent/.env \\
  -e NO_JAIL=1 \\
  --entrypoint safe-agent \\
  ghcr.io/pegasusheavy/safe-agent:latest`
		}
	];
</script>

<section id="quickstart" class="scroll-mt-20 py-20 md:py-28">
	<div class="mx-auto max-w-4xl px-6">
		<div class="mb-14 text-center">
			<h2 class="mb-4 text-3xl font-bold text-white md:text-4xl">Up and running in 3 steps</h2>
			<p class="mx-auto max-w-2xl text-slate-400">
				Pull the image, set two environment variables, and you're done.
				No Kubernetes. No Terraform. Just Docker.
			</p>
		</div>

		<div class="space-y-10">
			{#each steps as step}
				<div class="relative">
					<div class="mb-4 flex items-center gap-4">
						<span class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-accent/10 font-mono text-sm font-bold text-accent">
							{step.num}
						</span>
						<div>
							<h3 class="font-semibold text-white">{step.title}</h3>
							<p class="text-sm text-slate-400">{step.desc}</p>
						</div>
					</div>
					<CodeBlock code={step.code} title="terminal" />
				</div>
			{/each}
		</div>

		<div class="mt-12 rounded-xl border border-accent/20 bg-accent/5 p-6 text-center">
			<p class="text-sm text-slate-300">
				<i class="fa-solid fa-circle-info mr-2 text-accent"></i>
				Want Docker Compose instead? Check the
				<a
					href="https://github.com/pegasusheavy/safe-agent#docker-compose"
					target="_blank"
					rel="noopener"
					class="font-medium text-accent-light underline decoration-accent/30 underline-offset-2 hover:decoration-accent"
				>
					full docs
				</a>
				for a ready-to-use compose file.
			</p>
		</div>
	</div>
</section>
