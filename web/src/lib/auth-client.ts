import { createAuthClient } from 'better-auth/svelte';
import { api } from '$lib/api';

export const authClient = createAuthClient({
	// Set PUBLIC_API_URL in production (for example, https://api.example.com).
	// The local fallback keeps working through Vite's /api proxy.
	baseURL: api.authBaseUrl,
	fetchOptions: {
		credentials: 'include'
	}
});

export const { signIn, signUp, useSession, signOut } = authClient;
