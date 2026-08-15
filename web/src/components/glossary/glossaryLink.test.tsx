import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Glossed, glossLinks } from './glossaryLink';
import { Term } from './Term';

describe('Glossed', () => {
  it('passes a body with no terms through unchanged', () => {
    const text = 'Just some ordinary prose with no jargon at all.';
    render(<Glossed>{text}</Glossed>);

    expect(screen.queryAllByRole('button')).toHaveLength(0);
    expect(screen.getByText(text)).toBeInTheDocument();
  });

  it('matches whole words only, not substrings', () => {
    render(
      <Glossed>{'A cluster is not the same as clustered or binning.'}</Glossed>,
    );

    // "clustered" must not trigger cluster, "binning" must not trigger bin.
    expect(
      screen.getAllByRole('button', { name: 'Definition: cluster' }),
    ).toHaveLength(1);
    expect(
      screen.queryByRole('button', { name: 'Definition: bin' }),
    ).not.toBeInTheDocument();
  });

  it('links an automatic plural without needing it listed as an alias', () => {
    render(
      <Glossed>{'The clusters here outnumber the transactions.'}</Glossed>,
    );

    const link = screen.getByRole('button', { name: 'Definition: cluster' });
    expect(link).toHaveTextContent('clusters');
    // "transactions" is not a glossary term at all, so it stays plain text.
    expect(screen.getAllByRole('button')).toHaveLength(1);
    expect(screen.getByText(/outnumber the transactions/)).toBeInTheDocument();
  });

  it('prefers the longest match so a compound term wins over a term nested inside it', () => {
    render(<Glossed>{'A sigops-adjusted weight example.'}</Glossed>);

    expect(
      screen.getByRole('button', {
        name: 'Definition: sigops-adjusted weight',
      }),
    ).toHaveTextContent('sigops-adjusted weight');
    // The plain "weight" candidate must not fire separately inside the span already consumed.
    expect(screen.getAllByRole('button')).toHaveLength(1);
  });

  it('links only the first occurrence of a repeated term', () => {
    render(
      <Glossed>
        {
          'A cluster forms. Another cluster forms. A third cluster forms. cluster.'
        }
      </Glossed>,
    );

    expect(
      screen.getAllByRole('button', { name: 'Definition: cluster' }),
    ).toHaveLength(1);
  });

  it('matches case-insensitively while preserving the original casing in the rendered text', () => {
    render(<Glossed>{'Clusters are grouped by parent/child spends.'}</Glossed>);

    const link = screen.getByRole('button', { name: 'Definition: cluster' });
    expect(link).toHaveTextContent('Clusters');
  });

  it('never links the term whose own dialog is currently open', () => {
    render(
      <Glossed exclude="cluster">
        {'A cluster is a group of transactions.'}
      </Glossed>,
    );

    expect(
      screen.queryByRole('button', { name: 'Definition: cluster' }),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/A cluster is a group/)).toBeInTheDocument();
  });

  it('walks fragments and arrays, linking matches wherever they appear', () => {
    render(<Glossed>{['First cluster. ', 'Second cluster.']}</Glossed>);

    // Still only the first occurrence overall, even though it spans two array entries.
    expect(
      screen.getAllByRole('button', { name: 'Definition: cluster' }),
    ).toHaveLength(1);
    expect(screen.getByText(/Second cluster\./)).toBeInTheDocument();
  });

  it('keeps a wrapping element intact and links the text inside it', () => {
    const { container } = render(
      <Glossed>
        {['Read the ', <strong key="s">cluster</strong>, ' size.']}
      </Glossed>,
    );

    const strong = container.querySelector('strong');
    expect(strong).not.toBeNull();
    expect(
      screen.getByRole('button', { name: 'Definition: cluster' }),
    ).toBeInTheDocument();
    expect(strong?.querySelector('button')).not.toBeNull();
  });

  it('does not re-process text already inside an existing Term', () => {
    render(
      <Glossed>
        {[
          'See the ',
          <Term key="t" term="cluster">
            grouping
          </Term>,
          ' example.',
        ]}
      </Glossed>,
    );

    const links = screen.getAllByRole('button', {
      name: 'Definition: cluster',
    });
    expect(links).toHaveLength(1);
    expect(links[0]).toHaveTextContent('grouping');
  });
});

describe('glossLinks', () => {
  it('reports exactly the terms it linked, so a caller can drop them from a duplicate list', () => {
    const { linked } = glossLinks('A cluster forms in the mempool.');

    expect(linked).toEqual(new Set(['cluster', 'mempool']));
  });

  it('reports an empty set when nothing in the text matched', () => {
    const { linked } = glossLinks('Just some ordinary prose with no jargon.');

    expect(linked.size).toBe(0);
  });
});
