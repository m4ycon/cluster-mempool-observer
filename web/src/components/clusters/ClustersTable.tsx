import type { ClusterUpdate } from '../../hooks/useClusterDeltaSocket';
import { ClusterMetrics } from '../../lib/clusterMetrics';
import type { ClusterColumnKey } from '../../lib/clustersSearch';
import type { Column, SortState } from '../../lib/dataTable';
import { NumberFormat, TimeFormat } from '../../lib/format';
import type { ClusterRef } from '../../types/events';
import { Tooltip } from '../Tooltip';
import { DataTable } from '../table/DataTable';

export interface ClustersTableProps {
  clusters: ClusterRef[];
  lastUpdates: Map<number, ClusterUpdate>;
  sort: SortState<ClusterColumnKey>;
  onSortChange: (sort: SortState<ClusterColumnKey>) => void;
  page: number;
  onPageChange: (page: number) => void;
  query: string;
  onQueryChange: (query: string) => void;
  selectedId: number | null;
  onSelect: (id: number) => void;
}

const COLUMNS: readonly Column<ClusterRef, ClusterColumnKey>[] = [
  {
    key: 'id',
    label: 'ID',
    value: (c) => c.id,
    text: (c) => `#${c.id}`,
    align: 'left',
    searchable: true,
  },
  {
    key: 'txs',
    label: 'TXS',
    value: (c) => c.txids.length,
    align: 'right',
  },
  {
    key: 'fee',
    label: 'FEE (sats)',
    value: (c) => c.total_fee,
    text: (c) => NumberFormat.grouped(c.total_fee),
    align: 'right',
  },
  {
    key: 'vsize',
    label: 'VSIZE (vB)',
    value: (c) => c.total_vsize,
    text: (c) => NumberFormat.grouped(c.total_vsize),
    align: 'right',
  },
  {
    key: 'feerate',
    label: 'FEE-RATE (s/vB)',
    value: (c) => ClusterMetrics.value(c, 'feerate'),
    text: (c) => ClusterMetrics.value(c, 'feerate').toFixed(1),
    align: 'right',
  },
  {
    key: 'firstSeen',
    label: 'FIRST SEEN',
    value: (c) => TimeFormat.epoch(c.first_seen_at),
    text: (c) => TimeFormat.ago(c.first_seen_at),
    render: (c) => (
      <Tooltip label={TimeFormat.at(c.first_seen_at)} align="right">
        <span>{TimeFormat.ago(c.first_seen_at)}</span>
      </Tooltip>
    ),
    align: 'right',
  },
];

// Headline feature of this viz: find a cluster by any fragment of a txid it contains.
const searchExtra = (c: ClusterRef) => c.txids;

// Most clusters hold a single transaction, so any column ties in the thousands.
const tiebreak = (c: ClusterRef) => c.id;

/** Cluster-specific adapter over DataTable: columns, txid search, and the update-cue key. */
export function ClustersTable({
  clusters,
  lastUpdates,
  sort,
  onSortChange,
  page,
  onPageChange,
  query,
  onQueryChange,
  selectedId,
  onSelect,
}: ClustersTableProps) {
  const revisionOf = (id: number) => lastUpdates.get(id)?.revision ?? 0;

  // The revision is baked into the key so a re-mount replays the update cue.
  const rowKey = (c: ClusterRef) => `${c.id}:${revisionOf(c.id)}`;
  const selectedKey =
    selectedId != null ? `${selectedId}:${revisionOf(selectedId)}` : null;

  const rowClassName = (c: ClusterRef) =>
    lastUpdates.has(c.id) ? 'animate-row-flash' : undefined;

  return (
    <DataTable
      columns={COLUMNS}
      rows={clusters}
      rowKey={rowKey}
      sort={sort}
      onSortChange={onSortChange}
      page={page}
      onPageChange={onPageChange}
      query={query}
      onQueryChange={onQueryChange}
      searchExtra={searchExtra}
      tiebreak={tiebreak}
      selectedKey={selectedKey}
      onSelect={(c) => onSelect(c.id)}
      rowClassName={rowClassName}
      emptyLabel="awaiting cluster feed..."
      searchPlaceholder="search id, txid..."
    />
  );
}
