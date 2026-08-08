<script lang="ts">
	import { onMount } from 'svelte';
	let {
		matchId = '39',
		fieldNumber = 5,
		matchClock = 39,
		redScore = 16,
		blueScore = 16,
		globalScore = 0,
		redRoster = [],
		blueRoster = [],
		templateUrl
	}: {
		matchId?: string;
		fieldNumber?: number;
		matchClock?: number;
		redScore?: number;
		blueScore?: number;
		globalScore?: number;
		redRoster?: { name: string; id?: string; color?: string }[];
		blueRoster?: { name: string; id?: string; color?: string }[];
		templateUrl?: string;
	} = $props();

	let templateHtml = $state('');

	$effect(() => {
		if (templateUrl) {
			const fetchTemplate = async () => {
				try {
					const res = await fetch(templateUrl);
					if (res.ok) templateHtml = await res.text();
				} catch (e) {
					console.error('Failed to load scoreboard template', e);
				}
			};
			fetchTemplate();
		}
	});

	const formatMatchClock = (seconds: number) => {
		const wholeSeconds = Math.max(0, Math.ceil(seconds - 0.001));
		return `${Math.floor(wholeSeconds / 60)}:${String(wholeSeconds % 60).padStart(2, '0')}`;
	};
	const formattedClock = $derived(formatMatchClock(matchClock));

	function formatMatchNumber(id?: string): string {
		if (!id) return '39';
		if (id === 'test-match') return '39';
		if (id.length > 8) {
			const digits = id.replace(/\D/g, '');
			if (digits.length > 0) return digits.slice(0, 3);
			return id.slice(0, 4).toUpperCase();
		}
		return id;
	}
	const displayMatchNumber = $derived(formatMatchNumber(matchId));

	function getCountryCode(name?: string, fallback: string = ''): string {
		if (!name || name === '—') return fallback;
		const trimmed = name.trim().toUpperCase();
		if (trimmed.length === 3) return trimmed;
		const letters = trimmed.replace(/[^A-Z]/g, '');
		return letters.length >= 3 ? letters.slice(0, 3) : fallback || trimmed;
	}

	function renderFlag(code: string): string {
		const full = `style="width: 100%; height: 100%;"`;
		const relativeFull = `style="position: relative; width: 100%; height: 100%;"`;
		const flexCol = `style="width: 100%; height: 100%; display: flex; flex-direction: column;"`;
		const flexRow = `style="width: 100%; height: 100%; display: flex;"`;
		const flex1 = `style="flex: 1 1 0%;"`;
		const center = `display: flex; justify-content: center; align-items: center;`;

		if (code === 'LAO') return `<div ${relativeFull}><div style="background-color: #ce1126; width: 100%; height: 100%;"></div><div style="position: absolute; top: 6px; width: 100%; height: 12px; background-color: #002868; ${center}"><div style="width: 7px; height: 7px; background-color: white; border-radius: 9999px;"></div></div></div>`;
		if (code === 'ARM') return `<div ${flexCol}><div ${flex1} style="background-color: #d90012;"></div><div ${flex1} style="background-color: #0033a0;"></div><div ${flex1} style="background-color: #f28e00;"></div></div>`;
		if (code === 'BRU') return `<div ${relativeFull} style="background-color: #f7e017; overflow: hidden;"><div style="position: absolute; width: 150%; height: 10px; background-color: white; transform: rotate(22deg); top: 1px; left: -8px;"></div><div style="position: absolute; width: 150%; height: 10px; background-color: black; transform: rotate(22deg); top: 11px; left: -8px;"></div><div style="position: absolute; width: 10px; height: 10px; background-color: #ce1126; border-radius: 9999px; top: 6px; left: 13px; ${center}"><div style="width: 3px; height: 3px; background-color: #f7e017; border-radius: 9999px;"></div></div></div>`;
		if (code === 'TGA') return `<div ${relativeFull} style="background-color: #c10000;"><div style="position: absolute; top: 0; left: 0; width: 18px; height: 12px; background-color: white; ${center}"><div style="position: absolute; width: 10px; height: 3px; background-color: #c10000;"></div><div style="position: absolute; width: 3px; height: 10px; background-color: #c10000;"></div></div></div>`;
		if (code === 'SEY') return `<div ${relativeFull} style="overflow: hidden;"><div style="position: absolute; width: 100%; height: 100%; background-color: #007a3d; clip-path: polygon(0 100%, 100% 33%, 100% 100%);"></div><div style="position: absolute; width: 100%; height: 100%; background-color: white; clip-path: polygon(0 100%, 100% 0, 100% 33%);"></div><div style="position: absolute; width: 100%; height: 100%; background-color: #d62828; clip-path: polygon(0 100%, 66% 0, 100% 0);"></div><div style="position: absolute; width: 100%; height: 100%; background-color: #fcd856; clip-path: polygon(0 100%, 33% 0, 66% 0);"></div><div style="position: absolute; width: 100%; height: 100%; background-color: #003f87; clip-path: polygon(0 100%, 0 0, 33% 0);"></div></div>`;
		if (code === 'SUI') return `<div ${relativeFull} style="background-color: #d52b1e; ${center}"><div style="position: absolute; width: 16px; height: 5px; background-color: white;"></div><div style="position: absolute; width: 5px; height: 16px; background-color: white;"></div></div>`;
		if (code === 'VIE') return `<div ${relativeFull} style="background-color: #da251d; ${center}"><div style="color: #ffff00; font-size: 14px; line-height: 1; font-weight: bold;">★</div></div>`;
		if (code === 'USA') return `<div ${relativeFull} style="background-color: #b22234; display: flex; flex-direction: column; justify-content: space-between; overflow: hidden;"><div style="position: absolute; top: 0; left: 0; width: 16px; height: 12px; background-color: #3c3b6e; display: flex; align-items: center; justify-content: center; color: white; font-size: 7px;">★</div><div style="height: 3px; background-color: white; margin-top: 3px;"></div><div style="height: 3px; background-color: white;"></div><div style="height: 3px; background-color: white;"></div></div>`;
		if (code === 'JPN') return `<div ${relativeFull} style="background-color: white; ${center}"><div style="width: 12px; height: 12px; background-color: #bc002d; border-radius: 9999px;"></div></div>`;
		if (code === 'FRA') return `<div ${flexRow}><div ${flex1} style="background-color: #002654;"></div><div ${flex1} style="background-color: white;"></div><div ${flex1} style="background-color: #ce1126;"></div></div>`;
		if (code === 'GER') return `<div ${flexCol}><div ${flex1} style="background-color: black;"></div><div ${flex1} style="background-color: #dd0000;"></div><div ${flex1} style="background-color: #ffce00;"></div></div>`;
		return `<div ${full} style="background-color: #1e293b; ${center} color: white; font-size: 10px; font-weight: 900; letter-spacing: -0.05em;">${code.slice(0, 3)}</div>`;
	}

	const renderedHtml = $derived.by(() => {
		if (!templateHtml) return '';
		let html = templateHtml;
		html = html.replace('{{matchNumber}}', displayMatchNumber);
		html = html.replace('{{fieldNumber}}', String(fieldNumber || 5));
		html = html.replace('{{matchClock}}', formattedClock);
		html = html.replace('{{redScore}}', String(redScore));
		html = html.replace('{{blueScore}}', String(blueScore));

		const globalScoreBadge = globalScore > 0 ? `<div class="absolute z-[5] left-[304px] top-[206px] w-[120px] h-[18px] bg-black rounded-b-md flex items-center justify-center border-t border-[#d7ff7b]/40"><span class="font-sans text-[10px] font-bold text-[#d7ff7b]">EXT ${globalScore}</span></div>` : '';
		html = html.replace('{{{globalScoreBadge}}}', globalScoreBadge);

		const defaultRed = ['LAO', 'ARM', 'BRU'];
		const defaultBlue = ['TGA', 'SEY', 'SUI'];

		for (let i = 0; i < 3; i++) {
			const rCode = getCountryCode(redRoster[i]?.name, defaultRed[i]);
			html = html.replace(`{{redTeam${i}Code}}`, rCode);
			html = html.replace(`{{{redTeam${i}Flag}}}`, renderFlag(rCode));

			const bCode = getCountryCode(blueRoster[i]?.name, defaultBlue[i]);
			html = html.replace(`{{blueTeam${i}Code}}`, bCode);
			html = html.replace(`{{{blueTeam${i}Flag}}}`, renderFlag(bCode));
		}
		
		return html;
	});
</script>

{#if templateHtml}
	{@html renderedHtml}
{:else}
	<!-- Fallback loading or nothing while template is fetched -->
{/if}
