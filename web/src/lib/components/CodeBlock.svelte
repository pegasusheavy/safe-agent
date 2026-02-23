<script lang="ts">
	interface Props {
		code: string;
		lang?: string;
		title?: string;
	}

	let { code, lang = 'bash', title = '' }: Props = $props();
	let copied = $state(false);

	async function copy() {
		await navigator.clipboard.writeText(code);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}
</script>

<div class="group relative overflow-hidden rounded-xl border border-slate-800 bg-terminal-bg">
	{#if title}
		<div class="flex items-center gap-2 border-b border-slate-800 px-4 py-2">
			<span class="h-3 w-3 rounded-full bg-red-500/80"></span>
			<span class="h-3 w-3 rounded-full bg-yellow-500/80"></span>
			<span class="h-3 w-3 rounded-full bg-green-500/80"></span>
			<span class="ml-2 text-xs text-slate-500">{title}</span>
		</div>
	{/if}
	<div class="relative">
		<pre class="overflow-x-auto p-4 text-sm leading-relaxed"><code class="font-mono text-terminal-green">{code}</code></pre>
		<button
			onclick={copy}
			class="absolute top-3 right-3 rounded-md bg-slate-800/80 px-2.5 py-1.5 text-xs text-slate-400 opacity-0 transition-all hover:bg-slate-700 hover:text-slate-200 group-hover:opacity-100"
			aria-label="Copy code"
		>
			{#if copied}
				<i class="fa-solid fa-check text-accent"></i>
			{:else}
				<i class="fa-regular fa-copy"></i>
			{/if}
		</button>
	</div>
</div>
