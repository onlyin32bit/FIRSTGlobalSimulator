<script lang="ts">
	import type { TransferDebugEntry } from './match-protocol';

	let { entries, ballDebug }: { entries: TransferDebugEntry[], ballDebug?: Uint8Array } = $props();

	let stuckBalls = $derived.by(() => {
		if (!ballDebug) return [];
		let res = [];
		for (let i = 0; i < ballDebug.length; i++) {
			const flags = ballDebug[i];
			const insideRobot = (flags & 1) !== 0;
			if (insideRobot) {
				const touchingOuttake = (flags & 2) !== 0;
				const receivingForce = (flags & 4) !== 0;
				const contactCount = (flags >> 4) & 15;
				let status = "Transferring OK";
				let isStuck = false;
				
				if (touchingOuttake) {
					status = "At OuttakeZone (Transfer OFF)";
					isStuck = true;
				} else if (receivingForce && contactCount > 0) {
					status = `Colliding! (${contactCount} hits)`;
					isStuck = true;
				}
				
				res.push({ index: i, status, isStuck });
			}
		}
		return res;
	});

	type Row = { label: string; key: keyof TransferDebugEntry; hint: string };
	const ROWS: Row[] = [
		{
			label: 'Robot def loaded',
			key: 'hasRobotDefinition',
			hint: 'robot_definition is Some — a .fbx robot with semantic zones was registered'
		},
		{
			label: 'Transfer zone exists',
			key: 'hasTransferZone',
			hint: 'A TransferZone semantic was found in the robot definition'
		},
		{
			label: 'Ball detected',
			key: 'hasBall',
			hint: 'At least one active ball exists near this robot'
		},
		{
			label: 'Outtake / transfer power > 0',
			key: 'transferPowerOk',
			hint: 'max(intake_power, outtake_power) > 0 — intake or outtake button is pressed'
		},
		{
			label: 'Ball inside robot',
			key: 'insideRobot',
			hint: "Ball centre overlaps the robot bounds OBB"
		},
		{
			label: 'Ball touches OuttakeZone',
			key: 'touchesOuttake',
			hint: 'At least one active ball overlaps the authored OuttakeZone sensor'
		}
	];
</script>

<div class="transfer-debug-hud">
	<div class="hud-title">Transfer Debug</div>

	{#each entries as entry (entry.playerName)}
		<div class="player-block">
			<div class="player-header">
				<span class="player-name">{entry.playerName}</span>
				<span class="power-badges">
					<span class="badge" class:active={entry.intakePower > 0}>
						IN {entry.intakePower.toFixed(2)}
					</span>
					<span class="badge" class:active={entry.outtakePower > 0}>
						OUT {entry.outtakePower.toFixed(2)}
					</span>
				</span>
			</div>

			<table class="condition-table">
				<tbody>
					{#each ROWS as row}
						{@const value = entry[row.key] as boolean}
						<tr class="condition-row" class:pass={value} class:fail={!value} title={row.hint}>
							<td class="condition-icon">{value ? '✓' : '✗'}</td>
							<td class="condition-label">{row.label}</td>
						</tr>
					{/each}
				</tbody>
			</table>

			<div class="outtake-readout">
				force {entry.outtakeForceN.toFixed(1)} N · target {entry.outtakeTargetSpeedMps.toFixed(1)} m/s
				<br />
				zone contacts {entry.outtakeContactBalls} · fastest {entry.maxOuttakeContactSpeedMps.toFixed(2)} m/s
			</div>

			{#if ROWS.every((r) => entry[r.key] as boolean)}
				<div class="status-ok">Force should be applying</div>
			{:else}
				{@const failing = ROWS.filter((r) => !(entry[r.key] as boolean))}
				<div class="status-fail">Blocked by: {failing.map((r) => r.label).join(', ')}</div>
			{/if}
		</div>
	{/each}

	{#if entries.length === 0}
		<div class="no-players">No players in match</div>
	{/if}

	{#if stuckBalls.length > 0}
		<div class="player-block">
			<div class="hud-title">Balls in Storage</div>
			{#each stuckBalls as ball}
				<div class="ball-status {ball.isStuck ? 'status-fail' : 'status-ok'}">
					Ball {ball.index}: {ball.status}
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.transfer-debug-hud {
		position: fixed;
		top: 12px;
		right: 12px;
		z-index: 9999;
		font-family: 'JetBrains Mono', 'Fira Mono', monospace;
		font-size: 11px;
		display: flex;
		flex-direction: column;
		gap: 8px;
		pointer-events: none;
		max-width: 280px;
	}

	.hud-title {
		color: #94a3b8;
		font-size: 10px;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		padding: 0 2px 2px;
	}

	.player-block {
		background: rgba(2, 6, 23, 0.88);
		border: 1px solid rgba(100, 116, 139, 0.3);
		border-radius: 8px;
		padding: 8px 10px;
		backdrop-filter: blur(6px);
	}

	.player-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 6px;
		gap: 8px;
	}

	.player-name {
		color: #e2e8f0;
		font-weight: 600;
		font-size: 12px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.power-badges {
		display: flex;
		gap: 4px;
		flex-shrink: 0;
	}

	.badge {
		font-size: 9px;
		padding: 1px 5px;
		border-radius: 4px;
		background: rgba(51, 65, 85, 0.6);
		color: #64748b;
		transition: background 0.15s, color 0.15s;
	}

	.badge.active {
		background: rgba(99, 102, 241, 0.25);
		color: #a5b4fc;
	}

	.condition-table {
		width: 100%;
		border-collapse: collapse;
	}

	.condition-row {
		cursor: default;
	}

	.condition-icon {
		width: 16px;
		font-size: 11px;
		padding: 1px 4px 1px 0;
	}

	.condition-label {
		color: #94a3b8;
		padding: 1px 0;
	}

	.condition-row.pass .condition-icon {
		color: #4ade80;
	}

	.condition-row.pass .condition-label {
		color: #cbd5e1;
	}

	.condition-row.fail .condition-icon {
		color: #f87171;
	}

	.condition-row.fail .condition-label {
		color: #f87171;
	}

	.outtake-readout {
		margin-top: 6px;
		color: #cbd5e1;
		font-size: 10px;
		line-height: 1.4;
	}

	.status-ok {
		margin-top: 6px;
		color: #4ade80;
		font-size: 10px;
	}

	.status-fail {
		margin-top: 6px;
		color: #fbbf24;
		font-size: 10px;
		line-height: 1.4;
	}

	.no-players {
		color: #475569;
		font-size: 11px;
		padding: 4px;
	}

	.ball-status {
		margin-top: 4px;
		font-size: 10px;
	}
</style>
