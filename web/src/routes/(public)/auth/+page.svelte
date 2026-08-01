<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import * as Tabs from '$lib/components/ui/tabs';
	import { signIn, signUp } from '$lib/auth-client';

	let email = $state('');
	let password = $state('');
	let name = $state('');
	let team = $state('');
	let invitationCode = $state('');
	let errorMsg = $state('');
	let isSubmitting = $state(false);

	async function handleLogin(e: Event) {
		e.preventDefault();
		isSubmitting = true;
		errorMsg = '';
		try {
			const res = await signIn.email({
				email,
				password
			});
			if (res.error) {
				errorMsg = res.error.message || 'Login failed';
			} else {
				window.location.href = '/dashboard';
			}
		} catch (err: any) {
			errorMsg = err.message || 'An error occurred';
		} finally {
			isSubmitting = false;
		}
	}

	async function handleRegister(e: Event) {
		e.preventDefault();
		isSubmitting = true;
		errorMsg = '';
		try {
			const res = await signUp.email({
				email,
				password,
				name,
				team,
				invitationCode // Sent in body
			});
			if (res.error) {
				errorMsg = res.error.message || 'Registration failed';
			} else {
				window.location.href = '/dashboard';
			}
		} catch (err: any) {
			errorMsg = err.message || 'An error occurred';
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="h-screen w-full flex items-center justify-center bg-background p-4 relative">
	<div class="absolute inset-0 bg-[radial-gradient(circle_at_top,var(--color-primary)_0,transparent_50%)] opacity-10 pointer-events-none"></div>

	<Card.Root class="w-full max-w-md bg-card/60 backdrop-blur-xl border-border shadow-2xl relative z-10">
		<Card.Header class="text-center">
			<Card.Title class="text-2xl font-black tracking-tight text-primary">FGC 2026</Card.Title>
			<Card.Description>Sign in to enter the simulator.</Card.Description>
		</Card.Header>

		<Tabs.Root value="login" class="w-full">
			<Tabs.List class="grid w-full grid-cols-2 rounded-none border-b border-border bg-transparent">
				<Tabs.Trigger value="login" class="data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:shadow-none rounded-none">Login</Tabs.Trigger>
				<Tabs.Trigger value="register" class="data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:shadow-none rounded-none">Register</Tabs.Trigger>
			</Tabs.List>
			
			<Tabs.Content value="login">
				<form onsubmit={handleLogin} class="flex flex-col gap-4 p-6">
					{#if errorMsg}
						<div class="p-3 bg-red-950/50 border border-red-900 text-red-200 text-sm rounded-md">{errorMsg}</div>
					{/if}
					<div class="flex flex-col gap-2">
						<Label for="login-email">Email</Label>
						<Input id="login-email" type="email" bind:value={email} required />
					</div>
					<div class="flex flex-col gap-2">
						<Label for="login-password">Password</Label>
						<Input id="login-password" type="password" bind:value={password} required />
					</div>
					<Button type="submit" class="w-full mt-4" disabled={isSubmitting}>
						{isSubmitting ? 'Signing in...' : 'Sign In'}
					</Button>
				</form>
			</Tabs.Content>
			
			<Tabs.Content value="register">
				<form onsubmit={handleRegister} class="flex flex-col gap-4 p-6">
					{#if errorMsg}
						<div class="p-3 bg-red-950/50 border border-red-900 text-red-200 text-sm rounded-md">{errorMsg}</div>
					{/if}
					<div class="grid grid-cols-2 gap-4">
						<div class="flex flex-col gap-2">
							<Label for="reg-name">Full Name</Label>
							<Input id="reg-name" bind:value={name} required />
						</div>
						<div class="flex flex-col gap-2">
							<Label for="reg-team">Team Name</Label>
							<Input id="reg-team" bind:value={team} required placeholder="e.g. Team Vietnam" />
						</div>
					</div>
					<div class="flex flex-col gap-2">
						<Label for="reg-email">Email</Label>
						<Input id="reg-email" type="email" bind:value={email} required />
					</div>
					<div class="flex flex-col gap-2">
						<Label for="reg-password">Password</Label>
						<Input id="reg-password" type="password" bind:value={password} required />
					</div>
					<div class="flex flex-col gap-2">
						<Label for="reg-invite">Invitation Code</Label>
						<Input id="reg-invite" bind:value={invitationCode} required class="font-mono" />
						<p class="text-xs text-muted-foreground">Access requires a valid beta code.</p>
					</div>
					<Button type="submit" class="w-full mt-4" disabled={isSubmitting}>
						{isSubmitting ? 'Creating Account...' : 'Create Account'}
					</Button>
				</form>
			</Tabs.Content>
		</Tabs.Root>
	</Card.Root>
</div>
