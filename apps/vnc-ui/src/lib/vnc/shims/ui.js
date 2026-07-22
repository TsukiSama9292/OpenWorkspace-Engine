// Shim for KasmVNC UI dependencies
// These are UI-level features not needed for standalone VNC viewer

export const registeredWindows = new Map();
export const displayWindows = new Map();

export default {
  registeredWindows,
  displayWindows
};
