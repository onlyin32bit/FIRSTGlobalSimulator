import { writable } from 'svelte/store';

export type Alliance = 'red' | 'blue';
export type SlotController = 'human-drive' | 'ai-bot' | 'disabled';

export type RobotSpecs = {
  capacity: number;
  intakeRate: number;
  outtakeRate: number;
  outtakeAngle: number;
  outtakeVelocity: number;
  transferRate: number;
  transferHeight: number;
  transferVelocity: number;
  transferAngle: number;
  transferBurstMin: number;
  transferBurstMax: number;
};

export type RobotSlotConfig = {
  id: string;
  name: string;
  alliance: Alliance;
  spawnAnchor: string;
  controller: SlotController;
  specs: RobotSpecs;
};

export const defaultRobotSpecs: RobotSpecs = {
  capacity: 40,
  intakeRate: 5.0,
  outtakeRate: 2.0,
  outtakeAngle: 45,
  outtakeVelocity: 8,
  transferRate: 2.5,
  transferHeight: 0.2,
  transferVelocity: 5.0,
  transferAngle: 20,
  transferBurstMin: 3,
  transferBurstMax: 4,
};

export const defaultSlots: RobotSlotConfig[] = [
  {
    id: 'red-1',
    name: 'Red 1',
    alliance: 'red',
    spawnAnchor: 'redSpawn1',
    controller: 'human-drive',
    specs: { ...defaultRobotSpecs }
  },
  {
    id: 'red-2',
    name: 'Red 2',
    alliance: 'red',
    spawnAnchor: 'redSpawn2',
    controller: 'ai-bot',
    specs: { ...defaultRobotSpecs }
  },
  {
    id: 'red-3',
    name: 'Red 3',
    alliance: 'red',
    spawnAnchor: 'redSpawn3',
    controller: 'ai-bot',
    specs: { ...defaultRobotSpecs }
  },
  {
    id: 'blue-1',
    name: 'Blue 1',
    alliance: 'blue',
    spawnAnchor: 'blueSpawn1',
    controller: 'ai-bot',
    specs: { ...defaultRobotSpecs }
  },
  {
    id: 'blue-2',
    name: 'Blue 2',
    alliance: 'blue',
    spawnAnchor: 'blueSpawn2',
    controller: 'ai-bot',
    specs: { ...defaultRobotSpecs }
  },
  {
    id: 'blue-3',
    name: 'Blue 3',
    alliance: 'blue',
    spawnAnchor: 'blueSpawn3',
    controller: 'ai-bot',
    specs: { ...defaultRobotSpecs }
  }
];

export const matchSlotsStore = writable<RobotSlotConfig[]>(defaultSlots);
export const activeRobotSlotId = writable<string>('red-1');
export const humanPlayerAlliance = writable<Alliance>('red');
export const showRobotTagsStore = writable<boolean>(false);

export const robotStorageMap = writable<Record<string, number>>({
  'red-1': 0,
  'red-2': 0,
  'red-3': 0,
  'blue-1': 0,
  'blue-2': 0,
  'blue-3': 0
});

export const robotPhysicsState = writable({
  pos: { x: 0, y: 0, z: 0 },
  vel: { x: 0, y: 0, z: 0 },
  forward: { x: 0, y: 0, z: 0 },
  isIntakeActive: false,
  isShootActive: false,
  isTransferActive: false,
});

export const robotSpecs = writable<RobotSpecs>({ ...defaultRobotSpecs });
export const robotStorage = writable(0);
export const ballsInPlay = writable(1000);

// Sync active slot's storage and specs to robotStorage and robotSpecs for UI HUD compatibility
activeRobotSlotId.subscribe(($activeId) => {
  let slots: RobotSlotConfig[] = [];
  matchSlotsStore.subscribe((s) => (slots = s))();
  const activeSlot = slots.find((s) => s.id === $activeId);
  if (activeSlot) {
    robotSpecs.set(activeSlot.specs);
  }
  let map: Record<string, number> = {};
  robotStorageMap.subscribe((m) => (map = m))();
  robotStorage.set(map[$activeId] ?? 0);
});

robotStorageMap.subscribe(($map) => {
  let activeId = 'red-1';
  activeRobotSlotId.subscribe((id) => (activeId = id))();
  robotStorage.set($map[activeId] ?? 0);
});

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
