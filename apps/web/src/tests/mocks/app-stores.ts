import { writable } from 'svelte/store';

export const page = writable({
  url: new URL('http://localhost:5173/'),
  params: {}
});
