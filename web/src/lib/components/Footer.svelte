<script lang="ts">
	const year = new Date().getFullYear();
</script>

<footer class="border-t border-slate-800 bg-slate-950">
	<div class="mx-auto flex max-w-6xl flex-col items-center gap-6 px-6 py-12 md:flex-row md:justify-between">
		<div class="flex items-center gap-3">
			<span class="flex h-7 w-7 items-center justify-center rounded-md bg-accent font-mono text-xs font-black text-slate-950">SA</span>
			<span class="text-sm text-slate-500">&copy; {year} Pegasus Heavy Industries LLC</span>
		</div>

		<div class="flex items-center gap-6">
			<a
				href="https://github.com/pegasusheavy/safe-agent"
				target="_blank"
				rel="noopener"
				class="text-slate-500 transition-colors hover:text-slate-300"
				aria-label="GitHub"
			>
				<i class="fa-brands fa-github text-lg"></i>
			</a>
			<a
				href="https://www.patreon.com/c/PegasusHeavyIndustries?vanity=user"
				target="_blank"
				rel="noopener"
				class="text-slate-500 transition-colors hover:text-slate-300"
				aria-label="Patreon"
			>
				<i class="fa-brands fa-patreon text-lg"></i>
			</a>
			<a
				href="mailto:pegasusheavyindustries@gmail.com"
				class="text-slate-500 transition-colors hover:text-slate-300"
				aria-label="Email"
			>
				<i class="fa-solid fa-envelope text-lg"></i>
			</a>
		</div>
	</div>
</footer>
