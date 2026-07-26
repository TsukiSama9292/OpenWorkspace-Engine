import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { vncSettings } from '$lib/stores/vnc';

describe('vncSettings store', () => {
  beforeEach(() => {
    vncSettings.setQuality(5);
    vncSettings.setCompression(5);
    vncSettings.setViewOnly(false);
    vncSettings.setClipboardSync(true);
    vncSettings.setScaleViewport(true);
  });
  it('has default values', () => {
    const settings = get(vncSettings);
    expect(settings).toEqual({
      quality: 5,
      compression: 5,
      viewOnly: false,
      clipboardSync: true,
      scaleViewport: true
    });
  });

  it('updates quality', () => {
    vncSettings.setQuality(8);
    expect(get(vncSettings).quality).toBe(8);
  });

  it('updates compression', () => {
    vncSettings.setCompression(3);
    expect(get(vncSettings).compression).toBe(3);
  });

  it('updates viewOnly', () => {
    vncSettings.setViewOnly(true);
    expect(get(vncSettings).viewOnly).toBe(true);
  });

  it('updates clipboardSync', () => {
    vncSettings.setClipboardSync(false);
    expect(get(vncSettings).clipboardSync).toBe(false);
  });

  it('updates scaleViewport', () => {
    vncSettings.setScaleViewport(false);
    expect(get(vncSettings).scaleViewport).toBe(false);
  });

  it('preserves other fields when updating one', () => {
    vncSettings.setQuality(9);
    const settings = get(vncSettings);
    expect(settings.quality).toBe(9);
    expect(settings.compression).toBe(5);
    expect(settings.viewOnly).toBe(false);
  });
});
