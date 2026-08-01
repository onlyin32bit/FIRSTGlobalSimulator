import { writable } from 'svelte/store';

export interface ZoneAABB {
  id: string;
  min: [number, number, number];
  max: [number, number, number];
}

export const scores = writable({
  blueSU: 0,
  redSU: 0,
  blueFS: 0,
  redFS: 0,
  EXT: 0,
});

export function resetScores() {
  scores.set({ blueSU: 0, redSU: 0, blueFS: 0, redFS: 0, EXT: 0 });
}

/** Fire a scoring event. zone IDs map to the keys in field.semantics.json */
export function addScore(zoneId: string) {
  scores.update((s) => {
    switch (zoneId) {
      case 'blueSUscore': return { ...s, blueSU: s.blueSU + 1 };
      case 'redSUscore':  return { ...s, redSU:  s.redSU  + 1 };
      case 'blueFSscore': return { ...s, blueFS: s.blueFS + 1 };
      case 'redFSscore':  return { ...s, redFS:  s.redFS  + 1 };
      case 'EXTscore':    return { ...s, EXT:    s.EXT    + 1 };
      default:            return s;
    }
  });
}
