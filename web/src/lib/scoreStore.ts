import { writable } from 'svelte/store';

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
