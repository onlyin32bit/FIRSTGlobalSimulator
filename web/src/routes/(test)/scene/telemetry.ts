import { writable } from 'svelte/store';

export const robotTelemetry = writable({
  x: 0,
  y: 0,
  z: 0,
  speed: 0,
  turnRate: 0,
  accel: 0,
  fps: 0,
  forwardSpeed: 0,
  requestedForwardSpeed: 0,
  driveImpulse: 0,
  contactForce: 0,
  contactCount: 0,
  contacts: [] as string[],
  stuckTime: 0,
  autoUnstickCount: 0
});
