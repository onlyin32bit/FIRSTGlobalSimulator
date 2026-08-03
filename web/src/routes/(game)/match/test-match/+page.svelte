<script lang="ts">
	import { onMount } from 'svelte';
	import { Canvas, T } from '@threlte/core';
	import { Grid, HTML, OrbitControls } from '@threlte/extras';
	import { Button } from '$lib/components/ui/button';
	import { ApiError, api } from '$lib/api';

	type Player = {
		id: string;
		name: string;
		teamName: string;
		x: number;
		y: number;
		z: number;
		yaw: number;
		color: string;
	};
	let players = $state<Player[]>([]);
	let status = $state('Connecting…');
	let error = $state('');
	let localId = $state('');
	let socket: WebSocket | undefined;
	let sequence = 0;
	let pingNonce = 0;
	let pingMs = $state<number | null>(null);
	const pendingPings = new Map<number, number>();
	const pressed = new Set<string>();
	const inputKeys = new Set([
		'w',
		'a',
		's',
		'd',
		'arrowup',
		'arrowdown',
		'arrowleft',
		'arrowright'
	]);

	function sendInput() {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		const turn =
			Number(pressed.has('d') || pressed.has('arrowright')) -
			Number(pressed.has('a') || pressed.has('arrowleft'));
		const drive =
			Number(pressed.has('w') || pressed.has('arrowup')) -
			Number(pressed.has('s') || pressed.has('arrowdown'));
		socket.send(
			JSON.stringify({ type: 'input', sequence: ++sequence, move_x: turn, move_z: drive })
		);
	}
	function sendPing() {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		const nonce = ++pingNonce;
		pendingPings.set(nonce, performance.now());
		socket.send(JSON.stringify({ type: 'ping', nonce }));
	}

	onMount(async () => {
		const keydown = (event: KeyboardEvent) => {
			const key = event.key.toLowerCase();
			if (!inputKeys.has(key)) return;
			pressed.add(key);
			event.preventDefault();
			sendInput();
		};
		const keyup = (event: KeyboardEvent) => {
			const key = event.key.toLowerCase();
			if (!inputKeys.has(key)) return;
			pressed.delete(key);
			event.preventDefault();
			sendInput();
		};
		window.addEventListener('keydown', keydown);
		window.addEventListener('keyup', keyup);
		const inputTimer = window.setInterval(sendInput, 50);
		const pingTimer = window.setInterval(sendPing, 2000);
		try {
			const ticket = await api.createTestMatchTicket();
			localId = (await api.getCurrentUser()).user.id;
			socket = new WebSocket(ticket.ws_url);
			socket.onopen = () => {
				status = 'Connected';
				error = '';
				sendPing();
			};
			socket.onclose = (event) => {
				status = 'Disconnected';
				if (event.code !== 1000) error = `Match server closed the connection (code ${event.code}).`;
			};
			socket.onerror = () => {
				error = 'Unable to reach ws://localhost:3000. Start the Rust match server.';
			};
			socket.onmessage = (event) => {
				try {
					const message = JSON.parse(event.data);
					if (message.type === 'state') players = message.players;
					if (message.type === 'pong') {
						const started = pendingPings.get(message.nonce);
						if (started !== undefined) {
							const sample = performance.now() - started;
							pingMs = pingMs === null ? sample : pingMs * 0.7 + sample * 0.3;
							pendingPings.delete(message.nonce);
						}
					}
				} catch {}
			};
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to join the live test match.';
			status = 'Unavailable';
		}
		return () => {
			window.clearInterval(inputTimer);
			window.clearInterval(pingTimer);
			window.removeEventListener('keydown', keydown);
			window.removeEventListener('keyup', keyup);
			socket?.close();
		};
	});
</script>

<div class="relative h-[calc(100vh-3.5rem)] overflow-hidden bg-slate-950">
	<div
		class="absolute top-4 left-4 z-10 rounded-lg border border-white/15 bg-black/60 px-4 py-3 text-sm text-white backdrop-blur"
	>
		<p class="font-semibold">Live test match</p>
		<p class="mt-1 text-white/70">
			{status} · {players.length} player{players.length === 1 ? '' : 's'} · Ping: {pingMs === null
				? '—'
				: `${Math.round(pingMs)} ms`}
		</p>
		<p class="mt-2 text-xs text-white/60">W/S drive · A/D turn · Arrow keys also work</p>
		{#if error}<p class="mt-2 text-red-300">{error}</p>{/if}
	</div>
	<div class="absolute top-4 right-4 z-10">
		<Button
			href="/dashboard"
			variant="outline"
			class="border-white/20 bg-black/40 text-white hover:bg-white/10">Leave match</Button
		>
	</div>
	<Canvas
		><T.PerspectiveCamera makeDefault position={[11, 12, 14]} fov={50}
			><OrbitControls
				target={[0, 0, 0]}
				enablePan={false}
				minDistance={8}
				maxDistance={28}
			/></T.PerspectiveCamera
		><T.AmbientLight intensity={1.2} /><T.DirectionalLight
			position={[8, 12, 6]}
			intensity={2}
			castShadow
		/><T.Mesh position={[0, -0.25, 0]} receiveShadow
			><T.BoxGeometry args={[16, 0.5, 16]} /><T.MeshStandardMaterial color="#1e293b" /></T.Mesh
		><Grid
			cellColor="#475569"
			sectionColor="#64748b"
			cellSize={1}
			sectionSize={4}
			fadeDistance={30}
		/><T.Group
			>{#each players as player}<T.Group
					position={[player.x, player.y, player.z]}
					rotation={[0, player.yaw, 0]}
					><T.Mesh castShadow
						><T.BoxGeometry args={[0.76, 0.76, 0.76]} /><T.MeshStandardMaterial
							color={player.color}
							emissive={player.id === localId ? player.color : '#000000'}
							emissiveIntensity={player.id === localId ? 0.2 : 0}
						/></T.Mesh
					><HTML position={[0, 0.95, 0]} center
						><div
							class="pointer-events-none rounded bg-black/75 px-2 py-1 text-xs font-semibold whitespace-nowrap text-white shadow"
						>
							{player.name}
						</div></HTML
					></T.Group
				>{/each}</T.Group
		></Canvas
	>
</div>
