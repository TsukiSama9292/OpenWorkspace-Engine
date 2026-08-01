import { describe, it, expect, vi, afterEach } from 'vitest';
import { startKeepalive, tabHasFocus } from '$lib/keepalive/keepalive';

const originalVisibilityState = Object.getOwnPropertyDescriptor(document, 'visibilityState');

function stubActivity(visible: boolean, focused: boolean) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    value: visible ? 'visible' : 'hidden'
  });
  return vi.spyOn(document, 'hasFocus').mockReturnValue(focused);
}

function mockHeartbeatResponse(ok = true) {
  return {
    ok,
    status: ok ? 200 : 500,
    text: () => Promise.resolve(JSON.stringify({ status: ok ? 'ok' : 'error' }))
  };
}

function stubFetch() {
  const fetchMock = vi.fn().mockResolvedValue(mockHeartbeatResponse());
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function stubIframeFocused(focused: boolean) {
  const innerHasFocus = vi.fn(() => focused);
  const fakeIframe = {
    contentWindow: {
      document: { hasFocus: innerHasFocus }
    }
  };
  vi.spyOn(document, 'querySelectorAll').mockReturnValue(
    [fakeIframe] as unknown as NodeListOf<HTMLIFrameElement>
  );
  return innerHasFocus;
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  if (originalVisibilityState) {
    Object.defineProperty(document, 'visibilityState', originalVisibilityState);
  }
});

describe('tabHasFocus', () => {
  it('is true when the top document has focus', () => {
    stubActivity(true, true);
    expect(tabHasFocus()).toBe(true);
  });

  it('is true when the embedded iframe document has focus', () => {
    stubActivity(true, false);
    stubIframeFocused(true);
    expect(tabHasFocus()).toBe(true);
  });

  it('is true when the iframe element is the active element', () => {
    stubActivity(true, false);
    vi.spyOn(document, 'activeElement', 'get').mockReturnValue(document.createElement('iframe'));
    expect(tabHasFocus()).toBe(true);
  });

  it('is false when the page has no focus and no iframe is focused', () => {
    stubActivity(true, false);
    expect(tabHasFocus()).toBe(false);
  });
});

describe('keepalive', () => {
  it('sends a heartbeat immediately on start and then every 10 s while active', () => {
    vi.useFakeTimers();
    stubActivity(true, true);
    const fetchMock = stubFetch();

    const cleanup = startKeepalive('inst-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/instances/inst-1/heartbeat',
      expect.objectContaining({ method: 'POST', credentials: 'include' })
    );

    vi.advanceTimersByTime(10_000);
    expect(fetchMock).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(10_000);
    expect(fetchMock).toHaveBeenCalledTimes(3);

    cleanup();
  });

  it('does not send while blurred or hidden', () => {
    vi.useFakeTimers();
    const hasFocus = stubActivity(true, true);
    const fetchMock = stubFetch();

    const cleanup = startKeepalive('inst-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);

    hasFocus.mockReturnValue(false);
    window.dispatchEvent(new Event('blur'));
    vi.advanceTimersByTime(30_000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      value: 'hidden'
    });
    document.dispatchEvent(new Event('visibilitychange'));
    vi.advanceTimersByTime(30_000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    cleanup();
  });

  it('sends immediately on refocus after being inactive, then resumes cadence', () => {
    vi.useFakeTimers();
    const hasFocus = stubActivity(true, true);
    const fetchMock = stubFetch();

    const cleanup = startKeepalive('inst-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);

    hasFocus.mockReturnValue(false);
    window.dispatchEvent(new Event('blur'));
    vi.advanceTimersByTime(10_000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      value: 'hidden'
    });
    document.dispatchEvent(new Event('visibilitychange'));
    vi.advanceTimersByTime(10_000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      value: 'visible'
    });
    document.dispatchEvent(new Event('visibilitychange'));
    hasFocus.mockReturnValue(true);
    window.dispatchEvent(new Event('focus'));

    expect(fetchMock).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(10_000);
    expect(fetchMock).toHaveBeenCalledTimes(3);

    cleanup();
  });

  it('keeps sending heartbeats while focus is inside the embedded iframe', () => {
    vi.useFakeTimers();
    stubActivity(true, false);
    stubIframeFocused(true);
    const fetchMock = stubFetch();

    const cleanup = startKeepalive('inst-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(30_000);
    expect(fetchMock).toHaveBeenCalledTimes(4);

    cleanup();
  });

  it('stops heartbeats once focus leaves the embedded iframe', () => {
    vi.useFakeTimers();
    stubActivity(true, false);
    const innerHasFocus = stubIframeFocused(true);
    const fetchMock = stubFetch();

    const cleanup = startKeepalive('inst-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);

    innerHasFocus.mockReturnValue(false);
    document.dispatchEvent(new Event('focusout'));
    vi.advanceTimersByTime(30_000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    cleanup();
  });

  it('ignores a failed heartbeat and retries on the next tick', () => {
    vi.useFakeTimers();
    stubActivity(true, true);
    let fail = true;
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        fail
          ? Promise.resolve(mockHeartbeatResponse(false))
          : Promise.resolve(mockHeartbeatResponse(true))
      );
    vi.stubGlobal('fetch', fetchMock);

    const cleanup = startKeepalive('inst-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);

    fail = false;
    vi.advanceTimersByTime(10_000);
    expect(fetchMock).toHaveBeenCalledTimes(2);

    cleanup();
  });

  it('calls onHeartbeat after a successful heartbeat', async () => {
    stubActivity(true, true);
    stubFetch();
    const onHeartbeat = vi.fn();
    const cleanup = startKeepalive('inst-1', { onHeartbeat });

    expect(onHeartbeat).not.toHaveBeenCalled();
    await new Promise(r => setTimeout(r, 0));
    expect(onHeartbeat).toHaveBeenCalledTimes(1);
    expect(onHeartbeat).toHaveBeenCalledWith(expect.any(Number));
    cleanup();
  });

  it('does not call onHeartbeat when the heartbeat request fails', async () => {
    stubActivity(true, true);
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(mockHeartbeatResponse(false)));
    const onHeartbeat = vi.fn();
    const cleanup = startKeepalive('inst-1', { onHeartbeat });

    await new Promise(r => setTimeout(r, 0));
    expect(onHeartbeat).not.toHaveBeenCalled();
    cleanup();
  });

  it('calls onHeartbeat again on the next cadence tick', async () => {
    vi.useFakeTimers();
    stubActivity(true, true);
    stubFetch();
    const onHeartbeat = vi.fn();
    const cleanup = startKeepalive('inst-1', { onHeartbeat });

    await vi.advanceTimersByTimeAsync(10_000);
    expect(onHeartbeat).toHaveBeenCalledTimes(2);
    cleanup();
  });

  it('cleanup stops the interval and removes event listeners', () => {
    vi.useFakeTimers();
    const hasFocus = stubActivity(true, true);
    const fetchMock = stubFetch();

    const cleanup = startKeepalive('inst-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);

    cleanup();

    vi.advanceTimersByTime(100_000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    hasFocus.mockReturnValue(false);
    window.dispatchEvent(new Event('blur'));
    hasFocus.mockReturnValue(true);
    window.dispatchEvent(new Event('focus'));
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
