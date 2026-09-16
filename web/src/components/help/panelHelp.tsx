import type { ReactNode } from 'react';
import type { GlossaryTerm } from '../../lib/glossary';

export type HelpTopic =
  | 'clusters.circles'
  | 'clusters.treemap'
  | 'clusters.histogram'
  | 'clusters.table'
  | 'clusters.legend'
  | 'feerate.diagram'
  | 'clusterCount.overTime'
  | 'mempoolSize.overTime'
  | 'txsPerMin.overTime'
  | 'controls.bins'
  | 'feerate.window'
  | 'panel.selectedCluster'
  | 'panel.clusterDag'
  | 'panel.distribution';

/** Panel/control help copy, keyed by topic; shown in the help dialog via `<HelpButton>`. */
export const PANEL_HELP: Record<
  HelpTopic,
  { title: string; body: ReactNode; terms: GlossaryTerm[] }
> = {
  'clusters.circles': {
    title: 'Cluster graph',
    body: 'One bubble per cluster. Click a bubble to see details.',
    terms: ['cluster'],
  },
  'clusters.treemap': {
    title: 'Treemap',
    body: 'The same clusters as rectangles instead of circles.',
    terms: ['cluster'],
  },
  'clusters.histogram': {
    title: 'Cluster distribution',
    body: 'How many clusters fall into each range of the chosen metric. Unlike the other views this one covers every cluster in the mempool.',
    terms: ['bin', 'cluster'],
  },
  'clusters.table': {
    title: 'Cluster table',
    body: 'Every cluster in the mempool, sortable by any column and searchable by transaction id. Click a row to inspect it.',
    terms: ['cluster'],
  },
  'clusters.legend': {
    title: 'Colour tiers',
    body: 'Tiers are computed from the clusters currently on screen, so they shift as the mempool shifts. The same colour will not mean the same number ten minutes from now.',
    terms: [],
  },
  'feerate.diagram': {
    title: 'Mempool feerate diagram',
    body: (
      <span>
        Read it left to right as a miner filling blocks, best-paying
        transactions first. The x axis is block space used, the y axis is fees
        collected so far.{' '}
        <strong>The slope is the fee-rate at that point.</strong> A transaction
        that pays well for its size climbs the curve faster -- more fee in less
        space -- so it lands at the steep left end, while cheap ones stretch out
        flat to the right. That is why the curve can only bend down: once the
        best transactions are taken, what is left pays less. Dashed lines mark
        block boundaries. Hover anywhere to read the fee-rate right there.
      </span>
    ),
    terms: [
      'feerate-diagram',
      'marginal-fee-rate',
      'sigops-adjusted-weight',
      'block-boundary',
    ],
  },
  'clusterCount.overTime': {
    title: 'Cluster count over time',
    body: 'How many clusters the mempool held, sampled over the selected window. It moves with both arrivals and mining: a block wipes out the clusters it confirms.',
    terms: ['cluster'],
  },
  'mempoolSize.overTime': {
    title: 'Mempool size over time',
    body: 'How many unconfirmed transactions the node was holding, sampled over the selected window. Sharp drops are probably blocks.',
    terms: ['mempool'],
  },
  'txsPerMin.overTime': {
    title: 'Txs/min over time',
    body: 'Three counts of mempool activity, sampled over the selected window: transactions that arrived, transactions a block confirmed, and transactions that left the mempool without confirming. A gap in a line means that period was not measured, not that activity was zero.',
    terms: ['mempool'],
  },
  'controls.bins': {
    title: 'Bins',
    body: 'How finely the value range is sliced. Fewer bins give a smoother shape, more bins expose detail and noise.',
    terms: ['bin'],
  },
  'feerate.window': {
    title: 'Window',
    body: "Zoom to the first N blocks' worth of the queue, or show all of it. This only changes the view; nothing is filtered out of the mempool.",
    terms: ['block-boundary'],
  },
  'panel.selectedCluster': {
    title: 'Selected cluster',
    body: 'Everything the node knows about one cluster: its transactions, its total size and fee, the fee-rate a miner would earn by taking the whole group, and the transaction graph showing how they connect. Click a transaction id to copy it.',
    terms: ['fee-rate', 'vsize'],
  },
  'panel.clusterDag': {
    title: 'Transaction graph',
    body: (
      <span>
        Arrows point parent to child: the child spends an output of the parent
        transaction, which is exactly why they must be mined in that order, and
        why the whole group counts as one cluster. Grey outlined nodes are
        parents outside this cluster -- an already-confirmed transaction, or one
        belonging to another cluster; they are drawn from their txid alone and
        are never fetched, so they carry no fee or size. When a transaction has
        more than a handful of external parents, the extra ones collapse into a
        single node showing the count instead of cluttering the graph with
        dozens of stubs.{' '}
        <strong>
          A dashed outline means something about this transaction is still
          unresolved:
        </strong>{' '}
        it can show up in the mempool before we have retrieved all of it, and a
        background service fills that in shortly after, resolving the node. Size
        follows vsize, colour follows fee-rate, both read straight off the
        transaction. The panel shows a fitted preview -- open the expanded view
        to pan, zoom, and switch between horizontal and vertical layout.
      </span>
    ),
    terms: ['cluster', 'mempool', 'vsize', 'fee-rate'],
  },
  'panel.distribution': {
    title: 'Distribution',
    body: 'Summary of the metric the histogram is binning, across the whole mempool.',
    terms: ['p90'],
  },
};
