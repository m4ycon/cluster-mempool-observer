import { describe, expect, it } from 'vitest';
import { GLOSSARY, type GlossaryTerm } from '../../lib/glossary';
import { type HelpTopic, PANEL_HELP } from '../help/panelHelp';
import { glossLinks } from './glossaryLink';

/**
 * Drift guard. Auto-linking is global: a new term, or a new alias on an old
 * one, silently re-links copy written long before it existed -- possibly at the
 * wrong entry (a second "diagram" stealing the feerate one). These tables pin
 * what every body links today, so that change fails here instead of shipping.
 *
 * A diff means: confirm the new links point at the right entries, then update.
 */
const HELP_LINKS: Record<HelpTopic, GlossaryTerm[]> = {
  'clusters.circles': ['cluster'],
  'clusters.treemap': ['cluster'],
  'clusters.histogram': ['cluster', 'mempool'],
  'clusters.table': ['cluster', 'mempool'],
  'clusters.legend': ['cluster', 'mempool'],
  'feerate.diagram': ['block-boundary', 'fee-rate'],
  'clusterCount.overTime': ['cluster', 'mempool'],
  'mempoolSize.overTime': [],
  'controls.bins': ['bin'],
  'feerate.window': ['mempool'],
  'panel.selectedCluster': ['cluster', 'fee-rate'],
  'panel.distribution': ['cluster', 'mempool', 'p90'],
};

/** Same, for the definitions themselves; each excludes its own term. */
const GLOSSARY_LINKS: Record<GlossaryTerm, GlossaryTerm[]> = {
  cluster: [],
  mempool: [],
  vsize: [],
  weight: ['vsize'],
  'sigops-adjusted-weight': ['weight'],
  'fee-rate': ['satoshi', 'vsize'],
  'marginal-fee-rate': ['fee-rate'],
  satoshi: [],
  'feerate-diagram': ['fee-rate'],
  'block-boundary': ['weight'],
  bin: ['cluster'],
  p90: ['cluster'],
};

const linkedIn = (
  body: Parameters<typeof glossLinks>[0],
  self?: GlossaryTerm,
) => [...glossLinks(body, self).linked].sort();

describe('auto-linked terms', () => {
  it.each(
    Object.keys(HELP_LINKS) as HelpTopic[],
  )('links the expected terms in the %s help body', (topic) => {
    expect(linkedIn(PANEL_HELP[topic].body)).toEqual(
      [...HELP_LINKS[topic]].sort(),
    );
  });

  it.each(
    Object.keys(GLOSSARY_LINKS) as GlossaryTerm[],
  )('links the expected terms in the %s definition', (term) => {
    expect(linkedIn(GLOSSARY[term].text, term)).toEqual(
      [...GLOSSARY_LINKS[term]].sort(),
    );
  });
});
