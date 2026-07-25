import { writable } from 'svelte/store';

export const robotPhysicsState = writable({
  pos: { x: 0, y: 0, z: 0 },
  forward: { x: 0, y: 0, z: 0 },
  isIntakeActive: false,
  isShootActive: false,
});

export const robotSpecs = writable({
  capacity: 20,
  intakeRate: 5.0, // balls per sec
  outtakeRate: 2.0, // balls per sec
  outtakeAngle: 45, // degrees
  outtakeVelocity: 8.0, // m/s
});

export const robotStorage = writable(0);
