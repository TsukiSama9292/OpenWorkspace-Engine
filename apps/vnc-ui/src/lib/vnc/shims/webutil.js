// Shim for KasmVNC WebUtil dependencies

export function isInsideKasmVDI() {
  return false;
}

export function getCookie() {
  return '';
}

export function setCookie() {}

export default {
  isInsideKasmVDI,
  getCookie,
  setCookie
};
