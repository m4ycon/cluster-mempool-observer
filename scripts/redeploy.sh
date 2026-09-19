#!/usr/bin/env bash
# Re-deploys the prod stack from the working tree as it stands right now.
#
# What it adds over a bare `docker compose ... up -d --build`: the build is
# stamped with the commit it came from (compose defaults GIT_SHA to "unknown",
# which then lands in every server_started event), .env is checked before
# anything is torn down, and the api that comes back is confirmed to be the
# build this run produced.
#
# It deploys the current checkout and never pulls -- fetching is your call.
set -euo pipefail

cd "$(dirname "$0")/.."

# A cold build compiles the api from scratch before the container even starts.
readonly HEALTH_TIMEOUT_SECS=300

# Where compose.yml mounts ./data/node inside the node container.
readonly NODE_DATADIR=/home/bitcoin/.bitcoin

# Vars that must carry a value, not merely be present. Everything else in
# .env.example is checked for presence only, since compose defaults it.
readonly REQUIRED_NON_EMPTY=(
  PUBLIC_DOMAIN
  DB_USER DB_PASSWORD DB_NAME
  RPC_HOST RPC_USER RPC_PASS
  ZMQ_BLOCKS_ENDPOINT
)

db_external=false
allow_dirty=false

usage() {
  cat <<'USAGE'
Usage: scripts/redeploy.sh [--db-external] [--allow-dirty]

  --db-external   also apply compose.db-external.yml: publishes Postgres on the
                  public IP and provisions the read-only role. Leaving it off
                  closes that port again on the next deploy.
  --allow-dirty   deploy a dirty working tree anyway, stamping the build
                  <sha>-dirty so the recorded SHA does not claim more than it is.
USAGE
}

die() { echo "error: $*" >&2; exit 1; }
warn() { echo "warning: $*" >&2; }
step() { printf '\n==> %s\n' "$*"; }

while (($#)); do
  case $1 in
    --db-external) db_external=true ;;
    --allow-dirty) allow_dirty=true ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
  shift
done

compose_files=(-f compose.yml -f compose.prod.yml)
$db_external && compose_files+=(-f compose.db-external.yml)

compose() { docker compose "${compose_files[@]}" "$@"; }

# Assignment names in a file, `export` prefix and leading space tolerated.
env_keys() {
  sed -n 's/^[[:space:]]*\(export[[:space:]]\{1,\}\)\?\([A-Za-z_][A-Za-z0-9_]*\)=.*/\2/p' "$1" | sort -u
}

# Last assignment wins, as compose reads it. Strips an unquoted value's inline
# comment, surrounding quotes and CRLF endings, so the emptiness test sees what
# compose would see.
env_value() {
  sed -n "s/^[[:space:]]*\(export[[:space:]]\{1,\}\)\?$1=//p" .env \
    | tail -n 1 \
    | tr -d '\r' \
    | sed -e 's/[[:space:]]\{1,\}#.*$//' \
          -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' \
          -e 's/^"\(.*\)"$/\1/' -e "s/^'\(.*\)'\$/\1/"
}

check_env() {
  [[ -f .env ]] || die ".env not found. Copy .env.example to .env and fill it in."
  [[ -f .env.example ]] || die ".env.example not found, so .env cannot be checked against it."

  # PUBLIC_DOMAIN is required by compose.prod.yml but is deliberately absent
  # from .env.example, which describes the dev stack.
  local expected missing
  expected=$(printf '%s\nPUBLIC_DOMAIN\n' "$(env_keys .env.example)" | sort -u)
  missing=$(comm -23 <(printf '%s\n' "$expected") <(env_keys .env))

  local empty=() key
  local required=("${REQUIRED_NON_EMPTY[@]}")
  $db_external && required+=(DB_RO_PASSWORD)
  for key in "${required[@]}"; do
    grep -qx -- "$key" <<<"$missing" && continue
    [[ -n $(env_value "$key") ]] || empty+=("$key")
  done

  if [[ -n $missing || ${#empty[@]} -gt 0 ]]; then
    [[ -n $missing ]] && printf 'error: missing from .env: %s\n' "$(tr '\n' ' ' <<<"$missing" | sed 's/ *$//')" >&2
    ((${#empty[@]})) && printf 'error: set in .env but empty: %s\n' "${empty[*]}" >&2
    exit 1
  fi

  # Enabling the node container does not repoint the api: compose cannot branch
  # on a value, so these move together or the api talks to nothing.
  if [[ $(env_value NODE_ENABLED) == true ]]; then
    [[ $(env_value RPC_HOST) == node:* ]] \
      || die "NODE_ENABLED=true but RPC_HOST=$(env_value RPC_HOST); it should be node:8332."
    [[ $(env_value ZMQ_BLOCKS_ENDPOINT) == tcp://node:* ]] \
      || die "NODE_ENABLED=true but ZMQ_BLOCKS_ENDPOINT=$(env_value ZMQ_BLOCKS_ENDPOINT); it should be tcp://node:28333."
  fi
}

resolve_sha() {
  git rev-parse --git-dir >/dev/null 2>&1 || die "not a git repository, so the build cannot be stamped."

  local sha
  sha=$(git rev-parse --short HEAD)

  if [[ -n $(git status --porcelain) ]]; then
    $allow_dirty || die "the working tree is dirty, so $sha would not describe what gets built. Commit, stash, or pass --allow-dirty."
    warn "deploying a dirty working tree; stamping the build $sha-dirty."
    sha="$sha-dirty"
  fi

  local upstream ahead
  if upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null); then
    ahead=$(git rev-list --count "$upstream..HEAD")
    ((ahead)) && warn "$ahead commit(s) ahead of $upstream and not pushed."
  fi

  printf '%s' "$sha"
}

wait_for_api() {
  local cid status deadline=$((SECONDS + HEALTH_TIMEOUT_SECS))
  cid=$(compose ps -q api)
  [[ -n $cid ]] || die "no api container is running after the deploy."

  while :; do
    status=$(docker inspect -f '{{.State.Health.Status}}' "$cid" 2>/dev/null || echo unknown)
    case $status in
      healthy) return ;;
      unhealthy) die "the api came up unhealthy. Check: docker compose ${compose_files[*]} logs api" ;;
    esac
    ((SECONDS < deadline)) || die "the api was still '$status' after ${HEALTH_TIMEOUT_SECS}s."
    sleep 2
  done
}

verify_sha() {
  local running
  running=$(compose exec -T api printenv GIT_SHA 2>/dev/null | tr -d '\r\n') || true

  [[ -n $running ]] \
    || die "the api reports no GIT_SHA, so its image predates the build arg in docker/Dockerfile.api."
  [[ $running == "$1" ]] \
    || die "the api is running $running, not the $1 just built -- the container was not recreated."
}

# The node image's entrypoint chmods its datadir to 0700 and chowns it to its
# own uid on every start, undoing init-dirs, which ran earlier. The api runs as
# a different uid and then cannot even open data/node, so its disk sampler
# emits no `node` series at all. 0755 is all the sampler needs, and bitcoind
# owns the directory either way.
relax_node_datadir() {
  local cid
  cid=$(compose ps -q node)
  if [[ -z $cid ]]; then
    echo "the node container is not running"
    return
  fi

  if compose exec -T --user root node chmod 0755 "$NODE_DATADIR"; then
    echo "ok"
  else
    warn "could not chmod $NODE_DATADIR; the disk panels will be missing the node series."
  fi
}

docker compose version >/dev/null 2>&1 || die "docker compose is unavailable."

step "Checking .env"
check_env
echo "ok"

step "Resolving the commit to stamp"
git_sha=$(resolve_sha)
echo "$git_sha"

step "Building and starting the stack"
export GIT_SHA="$git_sha"
compose up -d --build

if $db_external; then
  step "Waiting for the read-only role provisioning"
  # `up -d` exits 0 even when provisioning failed; this is what reports it.
  compose wait db-provision-ro >/dev/null \
    || die "read-only role provisioning failed. Check: docker compose ${compose_files[*]} logs db-provision-ro"
  echo "ok"
fi

step "Waiting for the api to report healthy"
wait_for_api
echo "ok"

step "Verifying the running build"
verify_sha "$git_sha"
echo "api is running $git_sha"

step "Making data/node readable to the api"
relax_node_datadir

printf '\nDeployed %s to https://%s\n' "$git_sha" "$(env_value PUBLIC_DOMAIN)"
