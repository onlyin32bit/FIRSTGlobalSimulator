<script lang="ts">
	let {
		label,
		value,
		unit,
		samples,
		min = 0,
		max,
		reference,
		tone = 'text-cyan-300'
	}: {
		label: string;
		value: string;
		unit: string;
		samples: number[];
		min?: number;
		max: number;
		reference?: number;
		tone?: string;
	} = $props();

	const width = 184;
	const height = 28;
	const chartY = (sample: number) => {
		const normalized = (Math.min(max, Math.max(min, sample)) - min) / Math.max(max - min, 1);
		return height - normalized * height;
	};
	let points = $derived(
		samples
			.map((sample, index) => {
				const x = width - ((samples.length - 1 - index) * width) / 59;
				return `${x.toFixed(1)},${chartY(sample).toFixed(1)}`;
			})
			.join(' ')
	);
	let referenceY = $derived(reference === undefined ? null : chartY(reference));
</script>

<div class="grid grid-cols-[2.5rem_1fr] items-center gap-x-1 border-t border-white/8 py-1">
	<div class="leading-none">
		<div class="text-[9px] text-white/50">{label}</div>
		<div class={`mt-0.5 tabular-nums ${tone}`}>
			{value}<span class="ml-0.5 text-[8px] text-white/35">{unit}</span>
		</div>
	</div>
	<svg
		class="h-7 w-full overflow-visible"
		viewBox={`0 0 ${width} ${height}`}
		role="img"
		aria-label={`${label}, current ${value} ${unit}, 60 second history`}
	>
		<line
			x1="0"
			y1={height - 0.5}
			x2={width}
			y2={height - 0.5}
			stroke="currentColor"
			class="text-white/10"
		/>
		{#if referenceY !== null}
			<line
				x1="0"
				y1={referenceY}
				x2={width}
				y2={referenceY}
				stroke="currentColor"
				stroke-dasharray="2 3"
				class="text-white/20"
			/>
		{/if}
		{#if points}
			<polyline
				{points}
				fill="none"
				stroke="currentColor"
				stroke-width="1.5"
				vector-effect="non-scaling-stroke"
				class={tone}
			/>
		{/if}
	</svg>
</div>
