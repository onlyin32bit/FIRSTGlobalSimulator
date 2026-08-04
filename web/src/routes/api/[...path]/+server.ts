import type { RequestHandler } from './$types';

const HOP_BY_HOP_HEADERS = new Set([
	'connection',
	'keep-alive',
	'proxy-authenticate',
	'proxy-authorization',
	'te',
	'trailer',
	'transfer-encoding',
	'upgrade'
]);

function forward(request: Request, platform: App.Platform | undefined, preserveUpgrade = false) {
	if (!platform?.env.API) {
		return new Response('API service binding is unavailable.', { status: 503 });
	}

	const url = new URL(request.url);
	const headers = new Headers(request.headers);
	headers.delete('host');
	for (const header of HOP_BY_HOP_HEADERS) {
		if (preserveUpgrade && (header === 'upgrade' || header === 'connection')) continue;
		headers.delete(header);
	}

	return platform.env.API.fetch(
		new Request(`https://api.internal${url.pathname}${url.search}`, {
			method: request.method,
			headers,
			body: request.method === 'GET' || request.method === 'HEAD' ? undefined : request.body,
			redirect: 'manual'
		})
	);
}

export const GET: RequestHandler = ({ request, platform }) =>
	forward(request, platform, request.headers.get('upgrade')?.toLowerCase() === 'websocket');
export const POST: RequestHandler = ({ request, platform }) => forward(request, platform);
export const PUT: RequestHandler = ({ request, platform }) => forward(request, platform);
export const PATCH: RequestHandler = ({ request, platform }) => forward(request, platform);
export const DELETE: RequestHandler = ({ request, platform }) => forward(request, platform);
export const OPTIONS: RequestHandler = ({ request, platform }) => forward(request, platform);
