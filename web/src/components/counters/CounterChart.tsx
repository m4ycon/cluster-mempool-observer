import { useCounterSamples } from '../../hooks/useCounterSamples';
import { useSystemEvents } from '../../hooks/useSystemEvents';
import dayjs from '../../lib/dayjs';
import type { ChartRange } from '../../lib/routes';
import { CounterChartView } from './CounterChartView';

const RANGE_HOURS = 3;

/** Rounded to the minute so the fetch path stays stable across renders. */
function currentRange(): ChartRange {
  const to = dayjs().startOf('minute');
  return {
    from: to.subtract(RANGE_HOURS, 'hour').valueOf(),
    to: to.valueOf(),
  };
}

/** Fetches the counters series and lifecycle events, then renders `CounterChartView`. */
export function CounterChart() {
  const range = currentRange();
  const countersState = useCounterSamples(range);
  const eventsState = useSystemEvents(range);

  if (countersState.status === 'loading' || eventsState.status === 'loading') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-dim">
        loading sample history...
      </div>
    );
  }

  if (countersState.status === 'error') {
    return (
      <div className="flex h-full w-full items-center justify-center text-xs text-alert">
        failed to load sample history
      </div>
    );
  }

  // Markers are an overlay; a failed secondary request must not blank a working chart.
  const events = eventsState.status === 'loaded' ? eventsState.events : [];

  return (
    <CounterChartView
      series={countersState.series}
      range={range}
      events={events}
    />
  );
}
