// TODO: delete when it makes sense

// Deterministic client-side simulation for the Mempool Cluster Observer.
// Ported from the Claude Design source (Mempool Cluster Observer.dc.html) into
// a plain TypeScript class so the React pages can drive it. Everything here
// is pure/simulated -- no node connection. All figures are fabricated from
// seeded PRNGs so a given input always yields the same geometry.

export type FeeRange = '24h' | '7d' | '30d';
export type DistScale = 'log' | 'lin';
export interface Window {
  a: number;
  b: number;
}

export interface FeePoint {
  f: number;
  m: number;
  s: number;
}
export interface HistBar {
  s: string;
  c: number;
}
export interface ClusterCountPoint {
  c: number;
}

export interface Cluster {
  sz: number;
  fee: number;
  vs: number;
  id: string;
  seed: number;
  r: number;
  x: number;
  y: number;
}

export interface ClustersResult {
  count: number;
  mb: number;
  W: number;
  H: number;
  circles: Cluster[];
}

export interface SimNode {
  x: number;
  y: number;
  fee: number;
  rb: number;
}
export interface ClusterSim {
  nodes: SimNode[];
  links: [number, number][];
  maxDepth: number;
}

const NOW = Date.UTC(2026, 6, 1, 14, 23, 0); // 2026-07-01 14:23 UTC

export class Simulation {
  private _d?: { series: Record<FeeRange, FeePoint[]>; hist: HistBar[] };
  private _cl?: ClustersResult;
  private _sims: Record<number, ClusterSim> = {};
  private _cc?: ClusterCountPoint[];
  private _ms?: number[];
  private _fd?: number[];

  rng(seed: number): () => number {
    let a = seed >>> 0;
    return () => {
      a |= 0;
      a = (a + 0x6d2b79f5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  feeColor(f: number): string {
    return f < 6
      ? '#4d5a6b'
      : f < 14
        ? '#a05e17'
        : f < 28
          ? '#f7931a'
          : '#ffc46b';
  }

  data() {
    if (this._d) return this._d;
    const mk = (seed: number, n: number, waves: number): FeePoint[] => {
      const r = this.rng(seed);
      const pts: FeePoint[] = [];
      let v = 0;
      for (let i = 0; i < n; i++) {
        v = v * 0.92 + (r() - 0.5) * 2.6;
        const wave =
          5.5 * Math.sin((i / n) * Math.PI * 2 * waves + 1.3) +
          2.2 * Math.sin((i / n) * Math.PI * 2 * waves * 3.1);
        const spike = r() < 0.018 ? 8 + r() * 22 : 0;
        const med = Math.max(1.5, 16 + wave + v + spike);
        pts.push({
          f: med * (1.35 + r() * 0.35),
          m: med,
          s: Math.max(1, med * (0.5 + r() * 0.12)),
        });
      }
      return pts;
    };
    const series: Record<FeeRange, FeePoint[]> = {
      '24h': mk(11, 144, 1),
      '7d': mk(22, 168, 7),
      '30d': mk(33, 180, 30),
    };
    const hr = this.rng(44);
    const hist: HistBar[] = [];
    for (let s = 1; s <= 20; s++) {
      const c = Math.round(21000 * s ** -2.05 * (0.8 + hr() * 0.4));
      hist.push({ s: s === 20 ? '20+' : `${s}`, c: Math.max(3, c) });
    }
    this._d = { series, hist };
    return this._d;
  }

  clusterCount(propCount: number): number {
    return Math.max(5, Math.min(60, propCount ?? 40));
  }

  clusterCountSeries(): ClusterCountPoint[] {
    if (this._cc) return this._cc;
    const r = this.rng(8333);
    const n = 144; // 24h at the fee series' 10-min cadence
    let baseline = 32;
    let count = baseline;
    const pts: ClusterCountPoint[] = [];
    for (let i = 0; i < n; i++) {
      baseline += (r() - 0.5) * 0.5;
      count += 1.3 + r() * 2.4; // clusters pile up as new txs arrive
      // a block lands roughly every ~10 min and confirms away most of the backlog
      if (r() < 0.2) count = baseline + (count - baseline) * (0.2 + r() * 0.35);
      pts.push({ c: Math.max(3, count) });
    }
    this._cc = pts;
    return pts;
  }

  mempoolSizeSeries(): number[] {
    if (this._ms) return this._ms;
    const r = this.rng(21_000);
    const n = 144; // 24h at the fee series' 10-min cadence
    let baseline = 41_000;
    let size = baseline;
    const pts: number[] = [];
    for (let i = 0; i < n; i++) {
      baseline += (r() - 0.48) * 220;
      size += 300 + r() * 900; // arrivals outpace the drain between blocks
      // a block lands roughly every ~10 min and clears a few thousand txs
      if (r() < 0.2) size = baseline + (size - baseline) * (0.25 + r() * 0.4);
      pts.push(Math.max(1_000, size));
    }
    this._ms = pts;
    return pts;
  }

  /** Monotonic, concave cumulative-fee shape for the feerate-diagram preview: steep early, flattening. */
  feerateDiagramSeries(): number[] {
    if (this._fd) return this._fd;
    const r = this.rng(55_555);
    const n = 48;
    let cumulative = 0;
    const pts: number[] = [0];
    for (let i = 1; i < n; i++) {
      // early weight pays a higher marginal fee than the long tail, like a real fee diagram
      const marginal = Math.max(0.4, 6 * (1 - i / n) ** 1.6 + r() * 0.6);
      cumulative += marginal;
      pts.push(cumulative);
    }
    this._fd = pts;
    return pts;
  }

  clusters(propCount: number, moment: number): ClustersResult {
    const count = this.clusterCount(propCount);
    const mb = Math.round((moment ?? 1) * 24);
    if (this._cl && this._cl.count === count && this._cl.mb === mb)
      return this._cl;
    const r = this.rng(77 + mb * 97);
    const hex = '0123456789abcdef';
    const cs: Cluster[] = [];
    for (let i = 0; i < count; i++) {
      const sz = 1 + Math.floor(r() * r() * r() * 85);
      const fee = 2 + r() * 40;
      const vs = sz * (120 + Math.floor(r() * 380));
      let id = '';
      for (let k = 0; k < 8; k++) id += hex[Math.floor(r() * 16)];
      id += '…';
      for (let k = 0; k < 6; k++) id += hex[Math.floor(r() * 16)];
      cs.push({ sz, fee, vs, id, seed: 1000 + i * 13, r: 0, x: 0, y: 0 });
    }
    cs.sort((a, b) => b.vs - a.vs);
    const W = 720;
    const H = 560;
    const placed: Cluster[] = [];
    cs.forEach((c) => {
      c.r = 4 + Math.sqrt(c.sz) * 3.4;
      for (let t = 0; t < 3200; t++) {
        const th = t * 0.55;
        const d = t * 0.26;
        const x = W / 2 + Math.cos(th) * d;
        const y = H / 2 + Math.sin(th) * d * 0.8;
        if (x - c.r < 5 || x + c.r > W - 5 || y - c.r < 5 || y + c.r > H - 5)
          continue;
        let ok = true;
        for (const p of placed) {
          const dx = x - p.x;
          const dy = y - p.y;
          if (dx * dx + dy * dy < (c.r + p.r + 3) * (c.r + p.r + 3)) {
            ok = false;
            break;
          }
        }
        if (ok) {
          c.x = x;
          c.y = y;
          placed.push(c);
          break;
        }
      }
    });
    this._cl = { count, mb, W, H, circles: placed };
    this._sims = {};
    return this._cl;
  }

  clusterSim(idx: number, propCount: number, moment: number): ClusterSim {
    if (this._sims[idx]) return this._sims[idx];
    const c = this.clusters(propCount, moment).circles[idx];
    const r = this.rng(c.seed);
    const nodes: SimNode[] = [];
    const links: [number, number][] = [];
    const depth: number[] = [0];
    for (let i = 0; i < c.sz; i++) {
      nodes.push({
        x: 0.5 + (r() - 0.5) * 0.5,
        y: 0.5 + (r() - 0.5) * 0.5,
        fee: c.fee * (0.65 + r() * 0.7),
        rb: 2.2 + r() * 2.6,
      });
      if (i > 0) {
        const p = Math.floor(r() * i);
        links.push([p, i]);
        depth[i] = depth[p] + 1;
      }
    }
    const rest = Math.max(0.04, 0.35 / Math.sqrt(c.sz));
    for (let it = 0; it < 260; it++) {
      for (let i = 0; i < nodes.length; i++)
        for (let j = i + 1; j < nodes.length; j++) {
          const a = nodes[i];
          const b = nodes[j];
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const d2 = dx * dx + dy * dy + 1e-4;
          if (d2 < 0.06) {
            const f = 0.00025 / d2;
            a.x += dx * f;
            a.y += dy * f;
            b.x -= dx * f;
            b.y -= dy * f;
          }
        }
      links.forEach((l) => {
        const a = nodes[l[0]];
        const b = nodes[l[1]];
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const d = Math.sqrt(dx * dx + dy * dy) + 1e-6;
        const f = ((d - rest) * 0.14) / d;
        a.x += dx * f;
        a.y += dy * f;
        b.x -= dx * f;
        b.y -= dy * f;
      });
      nodes.forEach((n) => {
        n.x += (0.5 - n.x) * 0.01;
        n.y += (0.5 - n.y) * 0.01;
      });
    }
    let mnx = 1e9;
    let mny = 1e9;
    let mxx = -1e9;
    let mxy = -1e9;
    nodes.forEach((n) => {
      mnx = Math.min(mnx, n.x);
      mny = Math.min(mny, n.y);
      mxx = Math.max(mxx, n.x);
      mxy = Math.max(mxy, n.y);
    });
    const sx = Math.max(1e-6, mxx - mnx);
    const sy = Math.max(1e-6, mxy - mny);
    nodes.forEach((n) => {
      n.x = c.sz === 1 ? 0.5 : (n.x - mnx) / sx;
      n.y = c.sz === 1 ? 0.5 : (n.y - mny) / sy;
    });
    const sim: ClusterSim = { nodes, links, maxDepth: Math.max(...depth) };
    this._sims[idx] = sim;
    return sim;
  }

  feeView(feeRange: FeeRange, feeWin: Window) {
    const rn = feeRange;
    const pts = this.data().series[rn];
    const n = pts.length;
    const win = feeWin || { a: 0, b: 1 };
    const ia = Math.round(win.a * (n - 1));
    const ib = Math.max(ia + 1, Math.round(win.b * (n - 1)));
    const sub = pts.slice(ia, ib + 1);
    const w = 1000;
    const h = 420;
    const max = Math.max(...sub.map((p) => p.f)) * 1.08;
    const L = (k: keyof FeePoint) =>
      sub
        .map(
          (p, i) =>
            (i ? 'L' : 'M') +
            ((i / (sub.length - 1)) * w).toFixed(1) +
            ',' +
            (h - (p[k] / max) * h).toFixed(1),
        )
        .join('');
    const med = L('m');
    return {
      rn,
      n,
      ia,
      ib,
      sub,
      w,
      h,
      max,
      fast: L('f'),
      med,
      slow: L('s'),
      area: `${med}L${w},${h}L0,${h}Z`,
      maxLab: Math.round(max),
      midLab: Math.round(max / 2),
    };
  }

  distBarsWin(maxH: number, scale: DistScale, distWin: Window) {
    const hist = this.data().hist;
    const win = distWin || { a: 0, b: 1 };
    const width = Math.max(0.001, win.b - win.a);
    const jseed = Math.round(win.a * 200) * 31 + Math.round(win.b * 200);
    const jr = this.rng(1000000 + jseed);
    const counts = hist.map((b) =>
      Math.max(1, Math.round(b.c * width * (0.85 + jr() * 0.3))),
    );
    const mxC = Math.max(...counts);
    const mxL = Math.log10(mxC);
    const total = counts.reduce((s, c) => s + c, 0);
    return {
      total,
      bars: hist.map((b, i) => ({
        h: Math.max(
          2,
          Math.round(
            (scale === 'lin' ? counts[i] / mxC : Math.log10(counts[i]) / mxL) *
              maxH,
          ),
        ),
        lab: b.s,
        count: counts[i],
        i,
      })),
    };
  }

  feeAt(rn: FeeRange, w: number, h: number) {
    const pts = this.data().series[rn];
    const max = Math.max(...pts.map((p) => p.f)) * 1.08;
    const L = (k: keyof FeePoint) =>
      pts
        .map(
          (p, i) =>
            (i ? 'L' : 'M') +
            ((i / (pts.length - 1)) * w).toFixed(1) +
            ',' +
            (h - (p[k] / max) * h).toFixed(1),
        )
        .join('');
    const med = L('m');
    return {
      fast: L('f'),
      med,
      slow: L('s'),
      area: `${med}L${w},${h}L0,${h}Z`,
      maxLab: Math.round(max),
      midLab: Math.round(max / 2),
      spanLab: rn === '24h' ? '−24 h' : rn === '7d' ? '−7 d' : '−30 d',
    };
  }

  /** Line+area paths for `values` spread across a `w` x `h` box, headroom above the peak. */
  private sparklineAt(values: number[], w: number, h: number) {
    const max = Math.max(...values) * 1.08;
    const line = values
      .map(
        (v, i) =>
          (i ? 'L' : 'M') +
          ((i / (values.length - 1)) * w).toFixed(1) +
          ',' +
          (h - (v / max) * h).toFixed(1),
      )
      .join('');
    return { line, area: `${line}L${w},${h}L0,${h}Z` };
  }

  clusterCountAt(w: number, h: number) {
    return this.sparklineAt(
      this.clusterCountSeries().map((p) => p.c),
      w,
      h,
    );
  }

  mempoolSizeAt(w: number, h: number) {
    return this.sparklineAt(this.mempoolSizeSeries(), w, h);
  }

  feerateDiagramAt(w: number, h: number) {
    return this.sparklineAt(this.feerateDiagramSeries(), w, h);
  }

  hbars(maxH: number, scale: DistScale) {
    const hist = this.data().hist;
    const mxC = Math.max(...hist.map((b) => b.c));
    const mxL = Math.log10(mxC);
    return hist.map((b, i) => ({
      h: Math.max(
        2,
        Math.round(
          (scale === 'lin' ? b.c / mxC : Math.log10(b.c) / mxL) * maxH,
        ),
      ),
      lab: b.s,
      count: b.c,
      i,
    }));
  }

  timeLab(rn: FeeRange, f: number): string {
    if (rn === '24h') return `−${((1 - f) * 24).toFixed(1)} H`;
    if (rn === '7d') return `−${((1 - f) * 7).toFixed(1)} D`;
    return `−${((1 - f) * 30).toFixed(1)} D`;
  }

  // Absolute wall-clock timestamp for a slider fraction (f=1 is "now").
  // "now" is anchored to the observer's simulated snapshot time.
  tsLab(rn: FeeRange, f: number): string {
    const spanH = rn === '24h' ? 24 : rn === '7d' ? 24 * 7 : 24 * 30;
    const dt = new Date(NOW - (1 - f) * spanH * 3600 * 1000);
    const p = (x: number) => `${x}`.padStart(2, '0');
    return `${p(dt.getUTCMonth() + 1)}-${p(dt.getUTCDate())} ${p(dt.getUTCHours())}:${p(dt.getUTCMinutes())}`;
  }
}

// Single shared instance for the app -- deterministic, so any component can
// import it directly instead of threading it through props/context.
export const sim = new Simulation();
