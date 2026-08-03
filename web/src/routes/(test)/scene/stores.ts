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

export const humanPlayerThrow = writable({
  id: 0,
  origin: { x: 0, y: 1.8, z: 0 },
  direction: { x: 0, y: 0, z: -1 },
  power: 0
});

export const humanPlayerCharge = writable(0);
export const humanPlayerThrowMaxSpeed = writable(7.0);
export const humanPlayerGrabRequest = writable({
  id: 0,
  origin: { x: -4.41658, y: 1.8, z: 2.99308 },
  direction: { x: 0, y: 0, z: -1 }
});
export const humanPlayerStorage = writable(0);
export const humanPlayerHeldPosition = writable({ x: -4.41658, y: 1.8, z: 2.4 });

export type TargetedBallInfo = {
  id: number;
  x: number;
  y: number;
  z: number;
  screenX: number;
  screenY: number;
  visible: boolean;
};

export const humanPlayerTargetedBall = writable<TargetedBallInfo | null>(null);
