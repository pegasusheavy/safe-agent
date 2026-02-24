<script lang="ts">
	import Hero from '$lib/sections/Hero.svelte';
	import Features from '$lib/sections/Features.svelte';
	import QuickStart from '$lib/sections/QuickStart.svelte';
	import Architecture from '$lib/sections/Architecture.svelte';
	import SkillSystem from '$lib/sections/SkillSystem.svelte';
	import GetInvolved from '$lib/sections/GetInvolved.svelte';
</script>

<svelte:head>
	<title>safeclaw — Self-hosted AI agent on your hardware</title>
	<meta name="description" content="A sandboxed, self-hosted AI agent with tool execution, a skill system, knowledge graph, and multi-interface control. Your hardware. Your rules." />
</svelte:head>

<Hero />

<div class="mx-auto max-w-6xl px-6">
	<hr class="border-slate-800/50" />
</div>

<Features />

<div class="mx-auto max-w-6xl px-6">
	<hr class="border-slate-800/50" />
</div>

<QuickStart />

<div class="mx-auto max-w-6xl px-6">
	<hr class="border-slate-800/50" />
</div>

<Architecture />

<div class="mx-auto max-w-6xl px-6">
	<hr class="border-slate-800/50" />
</div>

<SkillSystem />

<div class="mx-auto max-w-6xl px-6">
	<hr class="border-slate-800/50" />
</div>

<GetInvolved />
