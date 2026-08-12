import { describe, expect, it } from 'vitest';
import { resolutionLabel } from './resolutionLabel';

describe('resolutionLabel', () => {
  it('labels sub-minute resolutions in seconds', () => {
    expect(resolutionLabel(1)).toBe('1-second samples');
    expect(resolutionLabel(30)).toBe('30-second samples');
  });

  it('labels minute resolutions', () => {
    expect(resolutionLabel(60)).toBe('1-minute samples');
    expect(resolutionLabel(300)).toBe('5-minute samples');
  });

  it('labels hour resolutions', () => {
    expect(resolutionLabel(3600)).toBe('1-hour samples');
    expect(resolutionLabel(7200)).toBe('2-hour samples');
  });

  it('labels day resolutions', () => {
    expect(resolutionLabel(86400)).toBe('1-day samples');
    expect(resolutionLabel(172800)).toBe('2-day samples');
  });
});
