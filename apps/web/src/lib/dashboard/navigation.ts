const RAIL_COLLAPSED_KEY = 'ow-rail-collapsed';

export function loadRailCollapsed(store: Storage): boolean {
  return store.getItem(RAIL_COLLAPSED_KEY) === '1';
}

export function saveRailCollapsed(store: Storage, collapsed: boolean): void {
  store.setItem(RAIL_COLLAPSED_KEY, collapsed ? '1' : '0');
}
