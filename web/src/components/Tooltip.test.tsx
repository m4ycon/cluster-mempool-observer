import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Tooltip } from './Tooltip';

describe('Tooltip', () => {
  it('renders the trigger and labels it for screen readers', () => {
    render(
      <Tooltip label="STATS FEED · LIVE">
        <span data-testid="dot" />
      </Tooltip>,
    );

    const bubble = screen.getByRole('tooltip');
    expect(bubble).toHaveTextContent('STATS FEED · LIVE');
    expect(screen.getByTestId('dot')).toBeInTheDocument();

    const trigger = bubble.parentElement;
    expect(trigger).toHaveAttribute('aria-describedby', bubble.id);
  });

  it('stays hidden until hovered or focused', () => {
    render(
      <Tooltip label="CLUSTER FEED · LIVE">
        <span />
      </Tooltip>,
    );

    const bubble = screen.getByRole('tooltip');
    expect(bubble).toHaveClass('opacity-0');
    expect(bubble).toHaveClass('group-hover:opacity-100');
    expect(bubble).toHaveClass('group-focus-within:opacity-100');
  });

  it('pins the bubble to the right edge when asked', () => {
    render(
      <Tooltip label="STATS FEED · OFFLINE" align="right">
        <span />
      </Tooltip>,
    );

    expect(screen.getByRole('tooltip')).toHaveClass('right-0');
  });
});
