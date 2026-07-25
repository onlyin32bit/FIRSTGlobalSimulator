import { writable } from 'svelte/store';

export const robotTelemetry = writable({
  x: 0,
  y: 0,
  z: 0,
  speed: 0,
  turnRate: 0,
  accel: 0
});
