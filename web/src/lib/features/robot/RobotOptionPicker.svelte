<script lang="ts" generics="T extends string">
	let {
		label,
		options,
		value = $bindable(),
		description
	}: { label: string; options: readonly T[]; value: T; description: string } = $props();
</script>

<fieldset>
	<legend class="font-medium">{label}</legend>
	<p class="mt-1 text-sm text-muted-foreground">{description}</p>
	<div class="mt-3 grid gap-2 sm:grid-cols-2">
		{#each options as option (option)}
			<button
				type="button"
				class:selected={value === option}
				class="option"
				onclick={() => (value = option)}
				aria-pressed={value === option}
			>
				{option}
			</button>
		{/each}
	</div>
</fieldset>

<style>
	.option {
		border-radius: 0.6rem;
		background: var(--muted);
		padding: 0.7rem 0.8rem;
		text-align: left;
		font-size: 0.875rem;
		transition:
			background-color 160ms ease,
			color 160ms ease;
	}
	.option:hover {
		background: color-mix(in oklch, var(--accent) 65%, var(--muted));
	}
	.selected {
		background: color-mix(in oklch, var(--primary) 25%, var(--muted));
		color: var(--foreground);
	}
	.option:focus-visible {
		outline: 2px solid var(--ring);
		outline-offset: 2px;
	}
</style>
