<script lang="ts">
	import Field from '$lib/components/Field.svelte';
	let { assets }: { assets: { visual: string; physics: string; semantics: string } } = $props();
</script>

<Field assetUrls={assets} clientPhysics={false} />
