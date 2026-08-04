import { browser } from '$app/environment';
import { createAuthClient } from 'better-auth/svelte';
const authBaseUrl = browser
	? import.meta.env.DEV
		? 'http://localhost:8787/api/auth'
		: new URL('/api/auth', window.location.origin).toString()
	: 'http://localhost:8787/api/auth';

type RegistrationInput = {
	email: string;
	password: string;
	name: string;
	team: string;
	invitationCode: string;
};

export const authClient = createAuthClient({
	baseURL: authBaseUrl,
	fetchOptions: {
		credentials: 'include'
	}
});

export const { signIn, useSession, signOut } = authClient;

export const signUp = {
	email(input: RegistrationInput) {
		return authClient.signUp.email(input as Parameters<typeof authClient.signUp.email>[0]);
	}
};
