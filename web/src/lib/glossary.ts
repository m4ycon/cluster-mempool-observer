export type GlossaryTerm =
  | 'cluster'
  | 'mempool'
  | 'vsize'
  | 'weight'
  | 'sigops-adjusted-weight'
  | 'fee-rate'
  | 'marginal-fee-rate'
  | 'satoshi'
  | 'feerate-diagram'
  | 'block-boundary'
  | 'bin'
  | 'p90';

/** Jargon glossary: dialog title/body content keyed by term. */
export const GLOSSARY: Record<
  GlossaryTerm,
  { label: string; text: string; aliases?: string[] }
> = {
  cluster: {
    label: 'cluster',
    text: 'A set of transactions linked by parent/child spends. A miner cannot include a child without its parent, so the whole group is evaluated -- and priced -- together. A transaction with no unconfirmed relatives is a cluster of one, and most of the mempool looks like that at any moment.',
  },
  mempool: {
    label: 'mempool',
    text: "The set of unconfirmed transactions this node is holding, waiting to be mined. Every node keeps its own, so another node's mempool will not match exactly.",
  },
  vsize: {
    label: 'vsize',
    text: "A transaction's size for fee purposes, in virtual bytes (vB). Signature data counts as a quarter of a normal byte, so a segwit transaction's vsize is smaller than its real byte count. A block holds 1,000,000 vB.",
    aliases: ['virtual bytes', 'vB'],
  },
  weight: {
    label: 'weight (WU)',
    text: 'The raw unit block space is measured in. 4 WU = 1 vB, and a block holds 4,000,000 WU. Weight is the number the consensus rule actually uses; vsize is the same thing divided by four, kept around because fees were quoted per byte long before weight existed.',
    aliases: ['WU', 'weight units'],
  },
  'sigops-adjusted-weight': {
    label: 'sigops-adjusted weight',
    text: 'Weight, raised for transactions carrying many signature checks. Those cost validation time rather than space, so Bitcoin Core prices them as if they were larger and ranks transactions on this adjusted number. For ordinary transactions it is just the weight.',
    aliases: ['sigops-adjusted'],
  },
  'fee-rate': {
    label: 'fee-rate',
    text: "Fee divided by vsize: satoshis per virtual byte (sat/vB). Space in a block is what's scarce, so a small transaction paying 5,000 sats can outrank a large one paying 8,000 -- it buys the miner more fee per byte.",
    aliases: ['feerate', 'fee rate', 'sat/vB'],
  },
  'marginal-fee-rate': {
    label: 'marginal fee-rate',
    text: 'The fee-rate at one specific point on the diagram: what the next piece of block space is paying. Not the average of everything to its left, which is always higher.',
  },
  satoshi: {
    label: 'satoshi (sat)',
    text: 'The smallest unit of bitcoin. 1 BTC = 100,000,000 sats. Fees here are always in sats.',
    aliases: ['sats'],
  },
  'feerate-diagram': {
    label: 'feerate diagram',
    text: 'A curve of total fees collected against block space used, as a miner fills a block best-paying transactions first. Its slope at any point is the fee-rate there.',
  },
  'block-boundary': {
    label: 'block boundary',
    text: "Every 4,000,000 WU of the diagram is one block's worth of space. The dashed lines mark where one block would end and the next begins.",
    aliases: ['block boundaries'],
  },
  bin: {
    label: 'bin',
    text: 'One bar of the histogram: a range of values, and a count of how many clusters fall inside it. More bins means a finer, noisier picture.',
  },
  p90: {
    label: 'p90',
    text: 'The 90th percentile. Nine out of ten clusters are below this value.',
  },
};
