<script lang="ts">
	let { children } = $props();
</script>

<main id="main-content" class="mx-auto w-full max-w-7xl px-4 py-8 sm:px-6 sm:py-10">
	{@render children()}
</main>
