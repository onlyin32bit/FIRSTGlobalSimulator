<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { IconLogin, IconPlayerPlay, IconPlus, IconRobot, IconUsers } from '@tabler/icons-svelte';
	import AppShell from '$lib/components/app/AppShell.svelte';
	import ActionCard from '$lib/features/dashboard/ActionCard.svelte';
	import MatchDialog from '$lib/features/dashboard/MatchDialog.svelte';
	import { DashboardController } from '$lib/features/dashboard/dashboard-controller.svelte';

	const controller = new DashboardController();

	function joinMatch() {
		const path = controller.joinPath();
		if (path) void goto(path);
	}

	async function createMatch() {
		const path = await controller.createMatch();
		if (path) void goto(path);
	}
</script>

<AppShell>
	<section class="mx-auto max-w-3xl">
		<p class="text-sm font-medium text-primary">FGC 2026</p>
		<h1 class="mt-2 font-daybreaker text-4xl tracking-wide sm:text-5xl">Choose a session</h1>
		<p class="mt-3 max-w-xl text-muted-foreground">
			Practice alone, enter the open arena, or set up a private match with your team.
		</p>

		<div class="mt-8 grid gap-3 sm:grid-cols-2">
			<ActionCard
				featured
				href={resolve('/match/test-match')}
				icon={IconPlayerPlay}
				title="Practice"
				description="Drive on the field by yourself."
			/>
			<ActionCard
				href={resolve('/match/arena')}
				icon={IconUsers}
				title="Open arena"
				description="Join the always-on shared field."
			/>
			<ActionCard
				onclick={() => controller.open('join')}
				icon={IconLogin}
				title="Join a match"
				description="Enter a lobby code from a host."
			/>
			<ActionCard
				onclick={() => controller.open('create')}
				icon={IconPlus}
				title="Create a match"
				description="Set up a private eight-player lobby."
			/>
		</div>

		<a
			class="mt-8 inline-flex items-center gap-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
			href={resolve('/robot')}
		>
			<IconRobot class="size-4" /> Manage robot builds
		</a>
	</section>
</AppShell>

<MatchDialog {controller} onJoin={joinMatch} onCreate={createMatch} />
