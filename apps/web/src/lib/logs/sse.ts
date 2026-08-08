//! DOM-free SSE stream parser for the container-log endpoint. Maintains a
//! partial-line buffer across reader chunks (and multi-byte UTF-8 sequences
//! crossing chunk boundaries), so split `event:`/`data:` frames parse cleanly
//! without ever doing `decode(chunk).split('\n')`.

/** One fully-delimited SSE event (the `event:` name plus its `data:` payload). */
export interface SseEvent {
  event: string;
  data: string;
}

/**
 * Incremental SSE parser. Feed it text chunks with `push()`, then call
 * `drain()` to collect the events that completed in those chunks. On stream
 * close, call `flush()` to deliver a trailing event that was never terminated
 * by a blank line.
 */
export class SseParser {
  private buffer = '';
  private eventName = '';
  private dataLines: string[] = [];
  private pending: SseEvent[] = [];

  push(chunk: string): void {
    this.buffer += chunk;
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() ?? '';
    for (const line of lines) this.processLine(stripCarriageReturn(line));
  }

  drain(onEvent: (event: SseEvent) => void): void {
    for (const event of this.pending) onEvent(event);
    this.pending = [];
  }

  /** Flush a final unterminated line, then deliver everything still pending. */
  flush(onEvent: (event: SseEvent) => void): void {
    if (this.buffer.length > 0) {
      this.processLine(stripCarriageReturn(this.buffer));
      this.buffer = '';
    }
    if (this.dataLines.length > 0) this.dispatch();
    this.drain(onEvent);
  }

  private processLine(line: string): void {
    if (line === '') {
      this.dispatch();
    } else if (line.startsWith(':')) {
      // Comment line — ignored (used by some proxies as a keepalive).
    } else if (line.startsWith('event:')) {
      this.eventName = line.slice('event:'.length).trim();
    } else if (line.startsWith('data:')) {
      // SSE allows `data:` across several lines, joined with a newline.
      this.dataLines.push(line.slice('data:'.length).replace(/^ /, ''));
    }
    // Other fields (`id:`, `retry:`) are ignored — our endpoint sends none.
  }

  private dispatch(): void {
    if (this.dataLines.length === 0) return;
    this.pending.push({ event: this.eventName, data: this.dataLines.join('\n') });
    this.eventName = '';
    this.dataLines = [];
  }
}

function stripCarriageReturn(line: string): string {
  return line.endsWith('\r') ? line.slice(0, -1) : line;
}
