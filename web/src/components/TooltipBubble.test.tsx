import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { TooltipBubble } from './TooltipBubble';

describe('TooltipBubble', () => {
  it('renders the bubble look with no visibility opinion, for a caller managing its own hover state', () => {
    render(
      <TooltipBubble style={{ left: 10, top: 20 }}>
        unknown inputs
      </TooltipBubble>,
    );

    const bubble = screen.getByRole('tooltip');
    expect(bubble).toHaveTextContent('unknown inputs');
    expect(bubble).toHaveClass('border-line', 'bg-bg', 'text-dim');
    expect(bubble).not.toHaveClass('opacity-0');
    expect(bubble.style.left).toBe('10px');
  });
});
