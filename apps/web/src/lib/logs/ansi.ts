//! Pure ANSI SGR renderer for container logs. Converts the common escape
//! sequences (fg/bg color incl. 256-color and truecolor, bold, reset) into HTML
//! `<span style="...">` wrappers; unknown sequences and line controls are
//! stripped. DOM-free and unit-testable, mirroring `src/lib/chart/`.

const ESC = String.fromCharCode(27);
const ANSI_ESCAPE = new RegExp(`${ESC}\\[([0-9;]*)([A-Za-z])`, 'g');

const BASIC_COLORS: string[] = [
  '#000000', '#800000', '#008000', '#808000',
  '#000080', '#800080', '#008080', '#c0c0c0',
  '#808080', '#ff0000', '#00ff00', '#ffff00',
  '#0000ff', '#ff00ff', '#00ffff', '#ffffff'
];

interface Style {
  bold?: boolean;
  fg?: string;
  bg?: string;
}

function rgb(r: number, g: number, b: number): string {
  const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v)));
  return `rgb(${clamp(r)},${clamp(g)},${clamp(b)})`;
}

/** The standard xterm 256-color palette (16 basic + 6x6x6 cube + grayscale). */
function xterm256Color(n: number): string {
  if (n < 16) return BASIC_COLORS[n] ?? BASIC_COLORS[0];
  if (n < 232) {
    const v = n - 16;
    const ramp = (x: number) => (x === 0 ? 0 : 55 + x * 40);
    return rgb(ramp(Math.floor(v / 36)), ramp(Math.floor((v % 36) / 6)), ramp(v % 6));
  }
  const gray = 8 + (n - 232) * 10;
  return rgb(gray, gray, gray);
}

function escapeHtml(text: string): string {
  // `\r` is a line control (progress-bar overwrites); dropped here.
  return text
    .replace(/\r/g, '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function styleToCss(style: Style): string {
  const parts: string[] = [];
  if (style.bold) parts.push('font-weight:600');
  if (style.fg) parts.push(`color:${style.fg}`);
  if (style.bg) parts.push(`background-color:${style.bg}`);
  return parts.join(';');
}

function renderText(text: string, style: Style): string {
  const escaped = escapeHtml(text);
  const css = styleToCss(style);
  return css ? `<span style="${css}">${escaped}</span>` : escaped;
}

function applySgr(style: Style, paramsRaw: string): Style {
  const params = paramsRaw ? paramsRaw.split(';').map((p) => Number(p)) : [0];
  const next: Style = { ...style };
  let i = 0;
  while (i < params.length) {
    const p = params[i];
    if (p === 0) {
      next.bold = undefined;
      next.fg = undefined;
      next.bg = undefined;
    } else if (p === 1) {
      next.bold = true;
    } else if (p === 22) {
      next.bold = undefined;
    } else if (p === 39) {
      next.fg = undefined;
    } else if (p === 49) {
      next.bg = undefined;
    } else if (p >= 30 && p <= 37) {
      next.fg = BASIC_COLORS[p];
    } else if (p >= 40 && p <= 47) {
      next.bg = BASIC_COLORS[p - 10];
    } else if (p >= 90 && p <= 97) {
      next.fg = BASIC_COLORS[p - 82];
    } else if (p >= 100 && p <= 107) {
      next.bg = BASIC_COLORS[p - 92];
    } else if (p === 38 || p === 48) {
      const kind: 'fg' | 'bg' = p === 38 ? 'fg' : 'bg';
      const mode = params[i + 1];
      if (mode === 5) {
        next[kind] = xterm256Color(params[i + 2]);
        i += 2;
      } else if (mode === 2) {
        next[kind] = rgb(params[i + 2], params[i + 3], params[i + 4]);
        i += 4;
      }
    }
    i += 1;
  }
  return next;
}

/**
 * Render ANSI-colored log text to HTML. Only SGR (`m`) sequences change the
 * style; all other escape sequences (`K`, `H`, cursor moves, …) are stripped.
 */
export function ansiToHtml(input: string): string {
  let out = '';
  let style: Style = {};
  let cursor = 0;

  ANSI_ESCAPE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = ANSI_ESCAPE.exec(input)) !== null) {
    const [, params, code] = match;
    const text = input.slice(cursor, match.index);
    if (text) out += renderText(text, style);
    if (code === 'm') style = applySgr(style, params);
    cursor = match.index + match[0].length;
  }
  out += renderText(input.slice(cursor), style);
  return out;
}
