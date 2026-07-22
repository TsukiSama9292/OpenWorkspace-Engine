import { writable } from 'svelte/store';

function createThemeStore() {
  const { subscribe, set, update } = writable('dark');

  return {
    subscribe,
    toggle: () => update(t => t === 'dark' ? 'light' : 'dark'),
    set,
    init: () => {
      if (typeof window !== 'undefined') {
        const stored = localStorage.getItem('vnc-theme');
        if (stored === 'light' || stored === 'dark') {
          set(stored);
        }
        subscribe(t => {
          localStorage.setItem('vnc-theme', t);
          document.documentElement.setAttribute('data-theme', t);
        });
      }
    }
  };
}

export const theme = createThemeStore();
