#!/usr/bin/env bash
# test-all.sh — lance toutes les suites offline + Postgres (ignore) et dit si tout est bon
# Usage: ./test-all.sh [--verbose]
# Env: RUSTORM_TEST_DATABASE_PORT (défaut 5432), RUSTORM_TEST_DATABASE_USER (défaut rustorm)
set -uo pipefail

VERBOSE=0
if [[ "${1:-}" == "--verbose" || "${1:-}" == "-v" ]]; then VERBOSE=1; fi

PORT="${RUSTORM_TEST_DATABASE_PORT:-5432}"
USER="${RUSTORM_TEST_DATABASE_USER:-rustorm}"
DB="${RUSTORM_TEST_DATABASE_NAME:-postgres}"

# couleurs si terminal
if [[ -t 1 ]]; then GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'; RESET='\033[0m'; else GREEN=''; RED=''; YELLOW=''; CYAN=''; RESET=''; fi

ok()  { echo -e "${GREEN}✔ $*${RESET}"; }
fail(){ echo -e "${RED}✘ $*${RESET}"; }
info(){ echo -e "${CYAN}→ $*${RESET}"; }

STEPS=0
FAILS=0
FAILED_STEPS=""

run_step() {
  local label="$1"; shift
  STEPS=$((STEPS+1))
  info "[$STEPS] $label"
  echo "  $ $*"
  if [[ $VERBOSE -eq 1 ]]; then
    if "$@"; then ok "$label"; else
      rc=$?; fail "$label (code $rc)"; FAILS=$((FAILS+1)); FAILED_STEPS+="- $label\n"
    fi
  else
    if "$@" > /tmp/rustorm-test-all-$$.log 2>&1; then ok "$label"; else
      rc=$?; fail "$label (code $rc)"; [[ $VERBOSE -eq 0 ]] && tail -20 /tmp/rustorm-test-all-$$.log | sed 's/^/  | /'
      FAILS=$((FAILS+1)); FAILED_STEPS+="- $label\n"
    fi
  fi
  echo
}

echo "rustorm — test-all.sh (P0+P1 M5 + P2 + TEST-MATRIX-AC-JALONS)"
echo "port=$PORT user=$USER db=$DB branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?') head=$(git rev-parse --short HEAD 2>/dev/null || echo '?')"
echo

# 1) offline sans Postgres — toutes suites rapides (P0 24 + P1 8 + P2 7 + M5 + TEST-MATRIX-AC-JALONS 39/39)
run_step "cargo test --offline (sans Postgres)" cargo test --offline -- --nocapture

# 1a) benches offline (baseline p50/p95/p99 budgets)
run_step "cargo test --offline --benches -- benches baseline (offline)" cargo test --offline --benches -- --nocapture || run_step "cargo test --offline --bench baseline" cargo test --offline --bench baseline -- --nocapture

# 1b) vérif fine offline par test (permet de localiser P1 M5)
if [[ $FAILS -eq 0 ]]; then
  info "Vérif fine offline par test (--test)"
  for t in p0_cli p0_migrations p0_query p0_schema p1_agg_groupby p1_cli_advanced p1_decimal p1_introspection p1_json_filters p1_perf p1_prisma_import p1_relations_polish p1_m5_completion p2_client_example p2_composites p2_connectors p2_extensions p2_migrations p2_multifile p2_tooling p2_views cardinalities cardinalities_postgres composite_primary composite_postgres migrations migrations_postgres named_postgres named_relations nested_connect_or_create nested_create nested_create_postgres nested_disconnect_postgres nested_graph nested_set_ext nested_upsert nested_writes_postgres query postgres relations_postgres shadow_postgres performance_baseline test_matrix_ac_jalons prisma_diff multi_os_verification prisma_diff_postgres performance_postgres connector_postgres connector_sqlx connector_trait connector_tls security_fuzz security_postgres transactions transactions_postgres; do
    # certains tests n'existent qu'avec --features postgres, on teste sans feature d'abord
    if cargo test --offline --test "$t" -- --nocapture > /tmp/rustorm-test-all-$$.log 2>&1; then
      STEPS=$((STEPS+1)); info "[$STEPS]   --test $t (offline)"; ok "  --test $t"; echo
    else
      # si le test nécessite postgres feature, on réessaye avec feature mais sans --ignored (offline part)
      if cargo test --offline --features postgres --test "$t" -- --nocapture > /tmp/rustorm-test-all-$$.log 2>&1; then
        STEPS=$((STEPS+1)); info "[$STEPS]   --test $t (offline+postgres)"; ok "  --test $t"; echo
      else
        STEPS=$((STEPS+1)); fail "  --test $t (offline)"; FAILS=$((FAILS+1)); FAILED_STEPS+="- --test $t (offline)\n"; tail -20 /tmp/rustorm-test-all-$$.log | sed 's/^/  | /'; echo
      fi
    fi
  done
fi

# 2) tout Postgres — suites ignorées par défaut
export RUSTORM_TEST_DATABASE_PORT="$PORT"
export RUSTORM_TEST_DATABASE_USER="$USER"
export RUSTORM_TEST_DATABASE_NAME="$DB"

if command -v pg_isready >/dev/null 2>&1; then
  if pg_isready -h 127.0.0.1 -p "$PORT" -U "$USER" -d "$DB" -q 2>/dev/null || pg_isready -h 127.0.0.1 -p "$PORT" -q 2>/dev/null; then
    ok "Postgres joignable sur 127.0.0.1:$PORT"
  else
    echo -e "${YELLOW}⚠ Postgres non joignable sur 127.0.0.1:$PORT — les suites Postgres vont échouer (lance docker avec -e POSTGRES_HOST_AUTH_METHOD=trust si besoin)${RESET}"
  fi
  echo
fi

run_step "cargo test --offline --features postgres -- --ignored (toutes suites PG)" cargo test --offline --features postgres -- --ignored -- --nocapture

# 3) ciblé par fichier pour un rapport fin (même commande mais détaillé par --test)
if [[ $FAILS -eq 0 ]]; then
  info "Vérif fine par test Postgres (--test)"
  for t in cardinalities_postgres composite_postgres migrations_postgres named_postgres nested_create_postgres nested_disconnect_postgres nested_writes_postgres postgres relations_postgres shadow_postgres transactions_postgres connector_postgres connector_sqlx prisma_diff_postgres performance_postgres security_postgres; do
    run_step "  --test $t" cargo test --offline --features postgres --test "$t" -- --ignored --nocapture
  done
fi

echo "────────────────────────────────────────"
if [[ $FAILS -eq 0 ]]; then
  echo -e "${GREEN}Tous les tests sont bons — $STEPS étapes sans exception.${RESET}"
  rm -f /tmp/rustorm-test-all-$$.log
  exit 0
else
  echo -e "${RED}Échecs: $FAILS/$STEPS${RESET}"
  echo -e "$FAILED_STEPS"
  echo "Relance en verbeux: ./test-all.sh --verbose"
  echo "ou ciblé: RUSTORM_TEST_DATABASE_PORT=$PORT cargo test --offline --features postgres --test <nom> -- --ignored --nocapture"
  rm -f /tmp/rustorm-test-all-$$.log
  exit 1
fi
