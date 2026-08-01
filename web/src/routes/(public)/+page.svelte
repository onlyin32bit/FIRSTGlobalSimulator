<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Textarea } from '$lib/components/ui/textarea';
	import { api } from '$lib/api';

	let inviteEmail = $state('');
	let inviteName = $state('');
	let inviteTeam = $state('');
	let inviteMessage = $state('');
	let inviteStatus = $state<'idle' | 'loading' | 'success' | 'error'>('idle');
	let errorMessage = $state('');

	let isInviteDialogOpen = $state(false);

	async function requestInvite(e: Event) {
		e.preventDefault();
		inviteStatus = 'loading';
		errorMessage = '';

		try {
			await api.requestInvite({
				email: inviteEmail,
				name: inviteName,
				team: inviteTeam,
				message: inviteMessage
			});
			inviteStatus = 'success';
		} catch (err: any) {
			inviteStatus = 'error';
			errorMessage = err instanceof Error ? err.message : 'Network error.';
		}
	}
</script>

<div
	class="relative flex h-screen w-full flex-col items-center justify-center overflow-hidden bg-background text-center"
>
	<div
		class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_center,var(--color-primary)_0,transparent_70%)] opacity-[0.03]"
	></div>

	<!-- Team Vietnam decorative background images -->
	<div class="pointer-events-none absolute inset-0 overflow-hidden">
		<img
			src="/images/team-vietnam.svg"
			alt="Team Vietnam logo"
			class="absolute -top-16 -left-16 h-auto w-[400px] rotate-[-10deg] object-cover opacity-15 blur-[1px] transition-all duration-700 hover:opacity-30 hover:blur-none"
		/>
	</div>

	<div class="z-10 flex max-w-2xl flex-col items-center gap-6 px-4">
		<h1 class="text-6xl font-black tracking-tighter text-primary">FIRST Global Simulator</h1>
		<p class="text-xl leading-relaxed font-medium text-muted-foreground">
			Join Team Vietnam and the global robotics community to design, build, and simulate your 2026
			Igniting Innovation robot in a fully synchronized, physics-driven multiplayer environment.
		</p>
		<p class="max-w-xl text-sm leading-relaxed text-muted-foreground">
			Inspired by the creativity, collaboration, and competitive spirit of Team Vietnam.
		</p>

		<div class="mt-8 flex gap-4">
			<Button href="/dashboard" size="lg" class="px-8 text-lg font-bold">Enter Simulator</Button>
			<Button
				variant="outline"
				size="lg"
				class="px-8 text-lg font-bold"
				onclick={() => (isInviteDialogOpen = true)}>Request Invite</Button
			>
		</div>

		<div class="mt-16 flex w-full max-w-lg flex-col items-center gap-6 border-t border-border pt-8">
			<div
				class="flex cursor-default items-center justify-center gap-8 opacity-50 grayscale transition-all hover:opacity-100 hover:grayscale-0"
			>
				<img src="/images/first-global.webp" alt="FIRST Global Logo" class="h-12 object-contain" />
				<div class="h-1 w-1 rounded-full bg-border"></div>
				<img
				src="/images/team-vietnam.svg"
					alt="Team Vietnam Logo"
					class="h-12 rounded-md object-contain"
				/>
			</div>
			<div
				class="flex flex-wrap items-center justify-center gap-x-4 gap-y-2 text-sm font-medium text-muted-foreground"
			>
				<a
					class="transition-colors hover:text-primary"
					href="https://www.facebook.com/TeamVietnamFGC">Facebook</a
				>
				<a
					class="transition-colors hover:text-primary"
					href="https://www.instagram.com/teamvietnam.fgc/">Instagram</a
				>
				<a class="transition-colors hover:text-primary" href="mailto:vietnamteamfgc@gmail.com"
					>vietnamteamfgc@gmail.com</a
				>
			</div>
			<p class="text-xs font-medium text-muted-foreground">
				FIRST Global Simulator is a Team Vietnam community project and is <strong>NOT</strong>
				affiliated with, sponsored by, or endorsed by <em>FIRST</em>® or <em>FIRST</em>® Global.
			</p>
		</div>
	</div>

	<Dialog.Root bind:open={isInviteDialogOpen}>
		<Dialog.Content class="sm:max-w-md">
			<Dialog.Header>
				<Dialog.Title>Request Beta Invite</Dialog.Title>
				<Dialog.Description>
					Leave your contact info and we'll send you an invitation code when spots open up.
				</Dialog.Description>
			</Dialog.Header>

			{#if inviteStatus === 'success'}
				<div class="flex flex-col items-center gap-4 py-12 text-center">
					<div
						class="flex h-12 w-12 items-center justify-center rounded-full bg-green-500/20 text-green-500"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="24"
							height="24"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
							><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" /><polyline
								points="22 4 12 14.01 9 11.01"
							/></svg
						>
					</div>
					<p class="font-bold">Request Received!</p>
					<p class="text-sm text-muted-foreground">Keep an eye on your inbox.</p>
					<Button onclick={() => (isInviteDialogOpen = false)} class="mt-4">Close</Button>
				</div>
			{:else}
				<form onsubmit={requestInvite} class="flex flex-col gap-4 py-4">
					{#if inviteStatus === 'error'}
						<div class="rounded-md border border-red-900 bg-red-950/50 p-3 text-sm text-red-200">
							{errorMessage}
						</div>
					{/if}

					<div class="flex flex-col gap-2">
						<Label for="req-name">Full Name</Label>
						<Input id="req-name" bind:value={inviteName} required />
					</div>
					<div class="flex flex-col gap-2">
						<Label for="req-team">Team / Organization</Label>
						<Input id="req-team" bind:value={inviteTeam} required placeholder="e.g. Team Vietnam" />
					</div>
					<div class="flex flex-col gap-2">
						<Label for="req-email">Email Address</Label>
						<Input id="req-email" type="email" bind:value={inviteEmail} required />
					</div>
					<div class="flex flex-col gap-2">
						<Label for="req-msg">Additional Info (Optional)</Label>
						<Textarea
							id="req-msg"
							bind:value={inviteMessage}
							placeholder="How did you hear about us?"
							rows={3}
						/>
					</div>

					<Dialog.Footer class="mt-4">
						<Button type="submit" class="w-full" disabled={inviteStatus === 'loading'}>
							{inviteStatus === 'loading' ? 'Sending...' : 'Submit Request'}
						</Button>
					</Dialog.Footer>
				</form>
			{/if}
		</Dialog.Content>
	</Dialog.Root>
</div>
