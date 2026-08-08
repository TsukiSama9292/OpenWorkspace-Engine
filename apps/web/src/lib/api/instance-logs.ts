//! Container-log streaming glue: opens the SSE endpoint via the native fetch
//! streaming reader, runs the chunk stream through `SseParser` (partial-line
//! buffer across chunks), and invokes callbacks for `log` events, the terminal
//! `end` event, and non-200 responses surfaced as errors from the pre-flight
//! check (never parsed as stream data).

import { SseParser } from '$lib/logs/sse';
import type { ContainerLogLine } from '$lib/types';

export interface LogStreamCallbacks {
  onLog: (line: ContainerLogLine) => void;
  onEnd: (reason: string) => void;
  onError: (message: string) => void;
}

export interface LogStreamOptions {
  tail?: number;
  follow?: boolean;
}

export function logsErrorMessage(status: number): string {
  switch (status) {
    case 401:
      return 'Authentication required to view logs';
    case 403:
      return "You do not have access to this instance's logs";
    case 404:
      return 'Instance not found';
    default:
      return `Failed to load logs (HTTP ${status})`;
  }
}

/** Start streaming an instance's logs. Returns `abort()` to close the stream. */
export function streamInstanceLogs(
  instanceId: string,
  options: LogStreamOptions,
  callbacks: LogStreamCallbacks
): { abort: () => void } {
  const controller = new AbortController();

  (async () => {
    const params = new URLSearchParams();
    params.set('tail', String(options.tail ?? 200));
    params.set('follow', String(options.follow ?? true));

    try {
      const res = await fetch(`/api/instances/${instanceId}/logs?${params}`, {
        credentials: 'include',
        headers: { Accept: 'text/event-stream' },
        signal: controller.signal
      });
      if (!res.ok) {
        callbacks.onError(logsErrorMessage(res.status));
        return;
      }
      if (!res.body) {
        callbacks.onError('Log stream unavailable');
        return;
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      const parser = new SseParser();
      let endDelivered = false;

      const handle = (event: { event: string; data: string }): void => {
        if (event.event === 'log') {
          try {
            const payload = JSON.parse(event.data) as { stream?: string; text?: string };
            callbacks.onLog({
              stream: payload.stream === 'stderr' ? 'stderr' : 'stdout',
              text: payload.text ?? ''
            });
          } catch {
            callbacks.onLog({ stream: 'stdout', text: event.data });
          }
        } else if (event.event === 'end') {
          endDelivered = true;
          try {
            const payload = JSON.parse(event.data) as { reason?: string };
            callbacks.onEnd(payload.reason ?? 'eof');
          } catch {
            callbacks.onEnd('eof');
          }
        }
      };

      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        parser.push(decoder.decode(value, { stream: true }));
        parser.drain(handle);
      }
      parser.flush(handle);
      if (!endDelivered) callbacks.onEnd('eof');
    } catch (err) {
      if (controller.signal.aborted) return;
      callbacks.onError(err instanceof Error ? err.message : 'Log stream failed');
    }
  })();

  return { abort: () => controller.abort() };
}
