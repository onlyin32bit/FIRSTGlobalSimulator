import { writable } from 'svelte/store';

export const robotPhysicsState = writable({
  pos: { x: 0, y: 0, z: 0 },
  vel: { x: 0, y: 0, z: 0 },
  forward: { x: 0, y: 0, z: 0 },
  isIntakeActive: false,
  isShootActive: false,
  isTransferActive: false,
});

export const robotSpecs = writable({
  capacity: 40,
  intakeRate: 5.0, // balls per second
  outtakeRate: 2.0, // balls per second
  outtakeAngle: 45, // degrees
  outtakeVelocity: 8, // m/s
  transferRate: 2.5, // bursts per second
  transferHeight: 0.20, // meters above robot bottom
  transferVelocity: 5.0, // m/s
  transferAngle: 20, // degrees elevation
  transferBurstMin: 3,
  transferBurstMax: 4,
});

export const robotStorage = writable(0);
export const ballsInPlay = writable(1000);
