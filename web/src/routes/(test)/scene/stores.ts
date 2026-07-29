import { writable } from 'svelte/store';

export const robotPhysicsState = writable({
  pos: { x: 0, y: 0, z: 0 },
  vel: { x: 0, y: 0, z: 0 },
  forward: { x: 0, y: 0, z: 0 },
  isIntakeActive: false,
  isShootActive: false,
});

export const robotSpecs = writable({
  capacity: 40,
  intakeRate: 5.0, // balls per second
  outtakeRate: 2.0, // balls per second
  outtakeAngle: 45, // degrees
  outtakeVelocity: 8, // m/s
});

export const robotStorage = writable(0);
export const ballsInPlay = writable(1000);
