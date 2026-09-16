# Cluster Mempool Observer

A tool to observe and analyze the mempool of a Bitcoin core node.

The main reason why this exists is to collect and analyze the movements of transactions in the mempool -- as we don't have any kind of historical view of the mempool. That way we could try to understand the behavior of it, aiming to gather some insights about how could we improve the mempool and the transaction relay in Bitcoin Core.

> [!IMPORTANT]  
> This is still **experimental** (expect breaking changes), I'm still working on ensuring integrity and the correctness of the data collected. Currently, I'd say that it is already ok but there are some problematic edge cases that I need to cover.
>
> Also, there's a good way of asserting the correctness of the data collected to say with confidence that the project is working as expected and trust-worthy, but it is still not possible -- probably there'll be an issue with it.

## Setup

Note: if you just want to see it running, you can skip this section and use the docker compose.

I assume you have a working Rust toolchain, Node.js and pnpm installed. If not, see [rust-lang.org](https://rust-lang.org/tools/install/), [nodejs.org](https://nodejs.org/) and [pnpm.io](https://pnpm.io/installation).

Additionally, you need a [Postgres](https://www.postgresql.org/) database and a [Bitcoin Core](https://bitcoincore.org/) node (or use the docker compose).

First, copy `.env.example` to `.env` and fill in values. One `.env` at the repo root serves everything -- backend, frontend and docker compose. The frontend reads only its `VITE_`-prefixed vars from it. The most importants vars will be the database connection settings and the Bitcoin Core RPC/ZMQ endpoints.

Then, build backend and frontend:

```bash
cargo build --bin api # at the repo root
cd web && pnpm install
```

Now, you can run the backend and frontend in separate terminals. Note: Migrations will be applied automatically on the first run.

### Backend

```bash
cargo run --bin api # Run the API server
cargo nextest run --all-features # Run all tests
cargo llvm-cov nextest --all-features --html --open # Coverage
cargo fmt --all # Format
cargo clippy --all-targets --all-features # Lint
```

By default, locally the API is served at http://localhost:3333.

The server starts answering immediately, but it needs a node that is up and past initial block download before it can bootstrap. Until that finishes, data routes reply `503` and `GET /health` reports why:

```json
{"ready":false,"phase":"waiting_for_node",
 "node":{"reachable":false,"error":"...Connection refused..."}}
```

`phase` moves `waiting_for_node` -> `bootstrapping` -> `ready`. `/health` itself is always `200`, since it answers "is the process up", which is also what the container healthcheck asks.

#### Database-backed tests

The database-backed tests are gated behind `db_integration_tests` and need Postgres. Any run that selects them -- `--all-features` included -- brings up the compose `db-test` service on its own and gives each of nextest's parallel slots a freshly migrated database, so tests never collide and nothing accumulates between runs. A plain `cargo nextest run` selects none of them and needs no Docker at all.

They do need nextest: plain `cargo test` has no setup phase to prepare the databases, and fails with a message saying so. To point the suite at a Postgres you manage instead, export the server address and the boot step is skipped:

```bash
export TEST_PG_URL_BASE=postgres://postgres:postgres@127.0.0.1:5434
```

### Frontend (`web/`)

```bash
pnpm dev            # run vite dev server
pnpm build          # tsc -b && vite build
pnpm test           # run tests
pnpm check          # biome check (lint + format + plugins)
pnpm check:write    # same, applying safe fixes
```

By default, locally the frontend is served at http://localhost:3000.


## Docker

`compose.yml` at the repo root runs Postgres, the api and the web build behind nginx, plus two opt-in pieces. Dev only: no authentication anywhere, every port bound to `HOST_BIND_IP` (`127.0.0.1` by default).

```bash
cp .env.example .env
docker compose up -d
docker compose down
```

| What | Where |
| --- | --- |
| Web | http://localhost:3000 |
| API | http://localhost:3333 |
| Metrics (`/metrics`) | http://localhost:3334 |
| Prometheus / Grafana (`METRICS_ENABLED`) | http://localhost:9090 / http://localhost:3031 |
| Node RPC / ZMQ hashblock (`NODE_ENABLED`) | `localhost:8332` / `localhost:28333` |
| Postgres | `localhost:5433` |

### State lives in `data/`

Every container's persistent state is bind-mounted under the repo, one folder per service, instead of living in docker's volume store:

```
data/postgres     the database
data/node         bitcoind datadir (.bitcoin), with NODE_ENABLED
data/prometheus   the metrics series, with METRICS_ENABLED
data/grafana      grafana's own sqlite db, with METRICS_ENABLED
data/api/logs     api log files from the container
```

### Docker-only env vars

Everything comes from the repo-root `.env`. Container-internal ports are fixed constants; these only control what reaches the host.

| Var | Controls |
| --- | --- |
| `HOST_BIND_IP` | host IP the published ports bind to (`0.0.0.0` exposes the stack to your network) |
| `API_PORT`, `METRICS_PORT` | api and its metrics listener |
| `WEB_PORT` | nginx, and the vite dev server |
| `PROMETHEUS_PORT`, `GRAFANA_PORT` | observability |
| `NODE_RPC_PORT`, `NODE_ZMQ_PORT` | node RPC and its hashblock stream |
| `NODE_PRUNE_MB` | node's on-disk block budget |
| `DB_EXTERNAL_PORT`, `DB_RO_USER`, `DB_RO_PASSWORD` | external read-only access to the db, prod only -- see [External database access](#external-database-access) |
| `COMPOSE_PROFILES` | which optional containers start -- see below |

`DB_*` are shared: they provision the `db` container **and** assemble `DATABASE_URL`, which the app and the diesel CLI read.

### Toggleable containers

Both switches are single vars in `.env`, read by the app *and* used to build the compose profile list:

```
METRICS_ENABLED=true
NODE_ENABLED=false
COMPOSE_PROFILES=metrics-${METRICS_ENABLED},node-${NODE_ENABLED}
```

Enabling the node does not repoint the api automatically -- compose cannot branch on a value -- so flip these together:

```
RPC_HOST=node:8332
ZMQ_BLOCKS_ENDPOINT=tcp://node:28333
```

With the node off (the default) the api talks to a Bitcoin Core on your host. `RPC_HOST` and `ZMQ_BLOCKS_ENDPOINT` must then be an **IPv4 address a container can route to** -- the host gateway (`172.x.x.x`), not `127.0.0.1`.

The node container is pruned mainnet (`bitcoin/bitcoin:31.1`), it has rpc and zmq enabled.

Enabling it means a full initial sync: the whole chain gets downloaded regardless of `NODE_PRUNE_MB`, which caps only what is *kept*. That can be skipped by seeding `data/node` from a cleanly stopped, already-synced datadir (`blocks/`, `chainstate/`, optionally `mempool.dat`), then `sudo chown -R 101:101 data/node`.

### Metrics

Prometheus scrapes `api:3334` over the compose network -- to point it at an api running on the host instead, edit that target in `docker/prometheus/prometheus.yml` and set `METRICS_BIND=0.0.0.0:3334`. Grafana dashboards are provisioned read-only into the **Cluster Mempool Observer** folder, so UI edits are discarded on restart; edit `docker/grafana/dashboards/generate.py` and rerun it from the repo root.

### Known: noisy cold start

First boot against an empty database bootstraps the whole mempool (~24k transactions) and logs thousands of `failed to retrieve transaction, persisting hollow: ... Connection refused`. The same binary run on the host does this with zero errors, so it is specific to the container's path to Core and not yet explained. It is self-correcting -- later cycles refill the rows, and a restart against the warm database logs no errors.

## Deploy on a VPS

`compose.prod.yml` is an overlay on top of `compose.yml`: it puts an nginx edge in front of the stack as one public origin behind Cloudflare, and leaves dev untouched. **The API has no authentication at all**, same as dev -- the edge and the firewall rules below are what make that safe to expose.

### Prerequisites

- A domain managed on Cloudflare (the free plan is enough). See [Without Cloudflare](#without-cloudflare) if you'd rather not.
- A VPS with Docker and the Compose plugin installed.

### Cloudflare setup

1. An `A` record for your domain at the VPS IP, proxied (orange cloud).
2. SSL/TLS mode **Full (strict)**, and **Always Use HTTPS** on. That redirect is why the origin never opens port 80.
3. Create an [Origin CA certificate](https://developers.cloudflare.com/ssl/origin-configuration/origin-ca/) and save it on the VPS:

```
data/edge/certs/origin.pem   # certificate
data/edge/certs/origin.key   # private key
```

`data/` is gitignored. Origin CA certs are trusted by Cloudflare only, which is the right trust boundary here, and they last 15 years -- no renewal job.

### `.env`

Copy `.env.example` to `.env` as usual and set `PUBLIC_DOMAIN`. It is the only var the overlay adds, and compose hard-fails without it.

### Firewall

443 should be reachable from Cloudflare only. Otherwise anyone who learns the origin IP reaches the unauthenticated API directly. The `444` on a non-matching Host helps, but it is not the control.

Cloudflare documents the options -- IP allowlisting, Authenticated Origin Pulls, Tunnel -- under [protecting your origin server](https://developers.cloudflare.com/fundamentals/security/protect-your-origin-server/).

### Bring it up

```bash
docker compose -f compose.yml -f compose.prod.yml up -d --build
```

| What | Where |
| --- | --- |
| Web | `https://<domain>/` |
| API | `https://<domain>/api/` |
| Websocket | `wss://<domain>/ws` |
| Grafana (`METRICS_ENABLED`) | `https://<domain>/grafana`, anonymous Viewer, no login |
| Prometheus | not routed at the edge -- reach it with an SSH tunnel |
| Postgres | not published at all, unless you add the overlay below |

### External database access

`compose.db-external.yml` is an opt-in overlay: it publishes Postgres on the VPS's public IP and provisions the read-only role that external clients connect with. Without it, nothing but 443 is published.

Open the firewall before the port, not after.

**1. Credentials.** Set `DB_RO_USER` and a strong `DB_RO_PASSWORD` in `.env`. Compose refuses to read the overlay without the password.

**2. Source IP.** `ufw` does not govern a Docker-published port: the traffic is DNAT'd into `FORWARD` and never reaches `INPUT`. The rule goes in `DOCKER-USER`, where 443 is already limited to Cloudflare.

```bash
PG_ALLOWED_IP=203.0.113.7

sudo iptables -I DOCKER-USER -p tcp --dport 5432 -s "$PG_ALLOWED_IP" -j RETURN
sudo iptables -I DOCKER-USER -p tcp --dport 5432 ! -s "$PG_ALLOWED_IP" -j DROP
sudo ip6tables -I DOCKER-USER -p tcp --dport 5432 -j DROP  # no v6 mapping today; keeps it that way
sudo netfilter-persistent save
```

`--dport` is the container's 5432, never `DB_EXTERNAL_PORT`: `DOCKER-USER` runs after the DNAT. Confirm with `sudo iptables -L DOCKER-USER -n --line-numbers` that these land above the Cloudflare jump, and that it is scoped to 443.

**3. Deploy.**

```bash
docker compose -f compose.yml -f compose.prod.yml -f compose.db-external.yml up -d
docker compose -f compose.yml -f compose.prod.yml -f compose.db-external.yml wait db-provision-ro
```

`up -d` exits 0 even when provisioning fails, so the second command is the one that tells you. Dropping the third `-f` closes the port again on the next deploy.

The role gets `SELECT` and nothing else, 5 connections, and 60s/30s statement and idle-in-transaction timeouts, so an external query cannot stall the ingest. Rotate the password by changing `DB_RO_PASSWORD` and deploying again.

**4. Verify from outside the VPS**, never from the box itself:

```bash
nc -vz -w5 <vps-ip> 5433   # connects from the allowed IP, times out from anywhere else
PGPASSWORD=... psql -h <vps-ip> -p 5433 -U mempool_ro -d mempool -c '\dt'
```

Reach for `nc` first: a `psql` auth failure reads like a blocked port and is not.

### Without Cloudflare

Nothing here needs Cloudflare, but five things assume it. To serve a bare IP or your own TLS:

- `server_name ${PUBLIC_DOMAIN}` and the `444` catch-all in `docker/edge/nginx.conf`: a request to the bare IP matches neither and gets closed. Use `server_name _;` and drop the catch-all server block.
- The certificate. No domain means no publicly-trusted cert, so bring your own or switch that block to `listen 80;` and delete the three `ssl_*` lines.
- `VITE_WS_BASE_URL` in `compose.prod.yml`: `wss://` needs TLS, plain HTTP needs `ws://<host>`.
- The firewall. With nothing in front there is no narrower source to restrict 443 to, so the port is open to the whole internet with an unauthenticated API behind it.
- `docker/edge/cloudflare-ips.conf`. Drop the `include` from `nginx.conf` and the mount from `compose.prod.yml`, otherwise you are trusting `CF-Connecting-IP` from a peer that is not Cloudflare.

The rate limits need no change -- without the `real_ip` block they key on the peer address, which is the true client when nothing is in front.

### AI/LLM Disclaimer

Yes, AI/LLM was used to assist building this project. Although, most (or all) of the code was reviewed and understood by the author. A few parts were "lgtm" reviewed (e.g. dashboards from grafana), but the core functionality was line by line architected, implemented and reviewed -- that's not one more AI slop / one day vibe coded project on github.

Feel free to also use to assist you in your own development and understanding of the project. But please don't send a thousand lines PR without a good reason.
