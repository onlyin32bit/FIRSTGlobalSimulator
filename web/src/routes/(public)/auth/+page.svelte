<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import type { Pathname } from '$app/types';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import * as Tabs from '$lib/components/ui/tabs';
	import { signIn, signUp, useSession } from '$lib/auth-client';
	import { fly, fade } from 'svelte/transition';

	let email = $state('');
	let password = $state('');
	let name = $state('');
	let team = $state('');
	let invitationCode = $state('');
	let loginError = $state('');
	let registrationError = $state('');
	let activeTab = $state<'login' | 'register'>('login');
	let isSubmitting = $state(false);
	const session = useSession();

	function safeNext(): Pathname {
		const next = page.url.searchParams.get('next');
		return (next?.startsWith('/') && !next.startsWith('//') ? next : '/dashboard') as Pathname;
	}

	$effect(() => {
		if (!$session.isPending && $session.data) goto(resolve(safeNext()));
	});

	function messageFor(error: unknown) {
		if (
			error &&
			typeof error === 'object' &&
			'message' in error &&
			typeof error.message === 'string'
		) {
			return error.message;
		}
		return 'Something went wrong. Please try again.';
	}

	async function handleLogin(event: SubmitEvent) {
		event.preventDefault();
		isSubmitting = true;
		loginError = '';
		try {
			const result = await signIn.email({ email: email.trim(), password });
			if (result.error) {
				loginError = result.error.message || 'Unable to sign in with those credentials.';
				return;
			}
			await goto(resolve(safeNext()));
		} catch (error) {
			loginError = messageFor(error);
		} finally {
			isSubmitting = false;
		}
	}

	async function handleRegistration(event: SubmitEvent) {
		event.preventDefault();
		isSubmitting = true;
		registrationError = '';
		try {
			const result = await signUp.email({
				email: email.trim(),
				password,
				name: name.trim(),
				team: team.trim(),
				invitationCode: invitationCode.trim().toUpperCase()
			});
			if (result.error) {
				registrationError = result.error.message || 'Unable to create your account.';
				return;
			}
			await goto(resolve(safeNext()));
		} catch (error) {
			registrationError = messageFor(error);
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="relative grid min-h-screen lg:grid-cols-2">
	<div
		class="relative hidden overflow-hidden bg-zinc-900 p-10 text-white lg:flex lg:flex-col lg:justify-between"
	>
		<div
			class="absolute inset-0 bg-[radial-gradient(circle_at_top,var(--color-primary)_0,transparent_50%)] opacity-25"
		></div>
		<div class="relative">
			<p class="text-lg font-semibold">FIRST Global Simulator</p>
			<p class="mt-2 max-w-md text-sm text-zinc-300">
				Design, test, and compete with your 2026 Igniting Innovation robot.
			</p>
		</div>
		<p class="relative max-w-sm text-sm text-zinc-300">
			Access is managed through an invitation code issued by the simulator administrators.
		</p>
	</div>

	<div class="flex items-center justify-center p-4 lg:p-8">
		<div class="mx-auto flex w-full max-w-sm flex-col gap-6">
			<div class="text-center">
				<h1 class="text-2xl font-semibold tracking-tight">Welcome to FGSimulator</h1>
				<p class="mt-2 text-sm text-muted-foreground">Sign in or redeem an invitation code.</p>
			</div>

			<Tabs.Root bind:value={activeTab} class="w-full">
				<Tabs.List class="mb-6 grid w-full grid-cols-2">
					<Tabs.Trigger value="login" onclick={() => (loginError = '')}>Sign in</Tabs.Trigger>
					<Tabs.Trigger value="register" onclick={() => (registrationError = '')}
						>Register</Tabs.Trigger
					>
				</Tabs.List>

				<Tabs.Content value="login" class="mt-0">
					<div in:fly={{ y: 10, duration: 250 }} out:fade={{ duration: 150 }}>
						<form class="flex flex-col gap-4" onsubmit={handleLogin}>
							{#if loginError}
								<div
									class="rounded-md border border-destructive/30 bg-destructive/15 p-3 text-sm text-destructive"
									role="alert"
								>
									{loginError}
								</div>
							{/if}
							<div class="flex flex-col gap-2">
								<Label for="login-email">Email</Label>
								<Input
									id="login-email"
									type="email"
									autocomplete="email"
									placeholder="you@example.com"
									bind:value={email}
									required
								/>
							</div>
							<div class="flex flex-col gap-2">
								<Label for="login-password">Password</Label>
								<Input
									id="login-password"
									type="password"
									autocomplete="current-password"
									bind:value={password}
									required
								/>
							</div>
							<Button type="submit" class="mt-2 w-full" disabled={isSubmitting}
								>{isSubmitting ? 'Signing in…' : 'Sign in'}</Button
							>
						</form>
					</div>
				</Tabs.Content>

				<Tabs.Content value="register" class="mt-0">
					<div in:fly={{ y: 10, duration: 250 }} out:fade={{ duration: 150 }}>
						<form class="flex flex-col gap-4" onsubmit={handleRegistration}>
							{#if registrationError}
								<div
									class="rounded-md border border-destructive/30 bg-destructive/15 p-3 text-sm text-destructive"
									role="alert"
								>
									{registrationError}
								</div>
							{/if}
							<div class="grid grid-cols-2 gap-4">
								<div class="flex flex-col gap-2">
									<Label for="name">Name</Label><Input
										id="name"
										autocomplete="name"
										bind:value={name}
										required
									/>
								</div>
								<div class="flex flex-col gap-2">
									<Label for="team">Team</Label><Input
										id="team"
										autocomplete="organization"
										bind:value={team}
										required
									/>
								</div>
							</div>
							<div class="flex flex-col gap-2">
								<Label for="email">Email</Label><Input
									id="email"
									type="email"
									autocomplete="email"
									placeholder="you@example.com"
									bind:value={email}
									required
								/>
							</div>
							<div class="flex flex-col gap-2">
								<Label for="password">Password</Label><Input
									id="password"
									type="password"
									autocomplete="new-password"
									minlength={8}
									bind:value={password}
									required
								/>
							</div>
							<div class="flex flex-col gap-2">
								<Label for="invitation-code">Invitation code</Label>
								<Input
									id="invitation-code"
									class="font-mono uppercase"
									placeholder="ABC123"
									bind:value={invitationCode}
									required
								/>
								<p class="text-xs text-muted-foreground">
									Contact the simulator administrators if you need access.
								</p>
							</div>
							<Button type="submit" class="mt-2 w-full" disabled={isSubmitting}
								>{isSubmitting ? 'Creating account…' : 'Create account'}</Button
							>
						</form>
					</div>
				</Tabs.Content>
			</Tabs.Root>
		</div>
	</div>
</div>
