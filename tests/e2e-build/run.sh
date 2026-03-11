#!/usr/bin/env bash
#
# E2E build tests for all Rise build backends.
#
# Usage:
#   ./tests/e2e-build/run.sh                       # all backends
#   ./tests/e2e-build/run.sh docker:buildx pack     # specific backends
#   ./tests/e2e-build/run.sh --no-proxy             # skip proxy tests
#   RISE_BIN=./target/debug/rise ./tests/e2e-build/run.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixture"
RISE_BIN="${RISE_BIN:-cargo run --features cli --}"
MITMPROXY_CTR="rise-e2e-mitmproxy"
BASE_PORT=18000

PASSED=0
FAILED=0
NO_PROXY=false

ALL_BACKENDS=(docker:build docker:buildx docker:buildctl pack railpack:buildx railpack:buildctl)
REQUESTED_BACKENDS=()

# Parse arguments
for arg in "$@"; do
    case "$arg" in
        --no-proxy) NO_PROXY=true ;;
        *) REQUESTED_BACKENDS+=("$arg") ;;
    esac
done

if [[ ${#REQUESTED_BACKENDS[@]} -eq 0 ]]; then
    REQUESTED_BACKENDS=("${ALL_BACKENDS[@]}")
fi

# Tracking arrays for cleanup
CONTAINERS_TO_CLEAN=()
IMAGES_TO_CLEAN=()
TEMP_FILES_TO_CLEAN=()

cleanup() {
    echo ""
    echo "--- Cleaning up ---"
    for ctr in "${CONTAINERS_TO_CLEAN[@]}" "$MITMPROXY_CTR"; do
        docker rm -f "$ctr" 2>/dev/null || true
    done
    for img in "${IMAGES_TO_CLEAN[@]}"; do
        docker rmi -f "$img" 2>/dev/null || true
    done
    for f in "${TEMP_FILES_TO_CLEAN[@]}"; do
        rm -f "$f" 2>/dev/null || true
    done
}
trap cleanup EXIT

# --- Helper functions ---

log_pass() {
    echo "[PASS] $1"
    ((PASSED++))
}

log_fail() {
    echo "[FAIL] $1"
    ((FAILED++))
}

safe_name() {
    # Convert backend name to a safe container/image suffix: docker:buildx -> docker-buildx
    echo "$1" | tr ':' '-'
}

check_tool() {
    local tool="$1"
    if ! command -v "$tool" &>/dev/null; then
        echo "ERROR: Required tool '$tool' not found. Install it (e.g. via mise install) and try again."
        exit 1
    fi
}

wait_for_http() {
    local url="$1"
    local expected="$2"
    local timeout="${3:-30}"
    local elapsed=0
    while [[ $elapsed -lt $timeout ]]; do
        local body
        body=$(curl -s --max-time 2 "$url" 2>/dev/null) || true
        if [[ "$body" == "$expected" ]]; then
            return 0
        fi
        sleep 1
        ((elapsed++))
    done
    return 1
}

# Build with rise, capturing output
rise_build() {
    echo "  Running: $RISE_BIN build $*"
    # shellcheck disable=SC2086
    $RISE_BIN build "$@"
}

# --- Tool checks ---

check_tool docker
check_tool curl

# Check backend-specific tools
for backend in "${REQUESTED_BACKENDS[@]}"; do
    case "$backend" in
        pack) check_tool pack ;;
        railpack:*) check_tool railpack ;;
        *:buildctl|docker:buildctl) check_tool buildctl ;;
    esac
done

# --- Proxy helpers ---

start_mitmproxy() {
    docker rm -f "$MITMPROXY_CTR" 2>/dev/null || true
    echo "  Starting mitmproxy..."
    docker run -d --name "$MITMPROXY_CTR" -p 8080:8080 \
        mitmproxy/mitmproxy mitmdump --set ssl_insecure=true >/dev/null

    # Wait for mitmproxy to generate its CA cert (up to 15s)
    local attempts=0
    while ! docker exec "$MITMPROXY_CTR" test -f /root/.mitmproxy/mitmproxy-ca-cert.pem 2>/dev/null; do
        sleep 1
        ((attempts++))
        if [[ $attempts -ge 15 ]]; then
            echo "  ERROR: mitmproxy CA cert not generated after 15s"
            return 1
        fi
    done
}

get_combined_ca_bundle() {
    local tmp_ca
    tmp_ca=$(mktemp /tmp/rise-e2e-ca-XXXXXX.pem)
    TEMP_FILES_TO_CLEAN+=("$tmp_ca")

    # System CAs (try common locations)
    local system_ca=""
    for ca_path in /etc/ssl/certs/ca-certificates.crt /etc/pki/tls/certs/ca-bundle.crt /etc/ssl/cert.pem; do
        if [[ -f "$ca_path" ]]; then
            system_ca="$ca_path"
            break
        fi
    done

    if [[ -n "$system_ca" ]]; then
        cat "$system_ca" > "$tmp_ca"
    fi

    # Append mitmproxy CA
    docker exec "$MITMPROXY_CTR" cat /root/.mitmproxy/mitmproxy-ca-cert.pem >> "$tmp_ca"
    echo "$tmp_ca"
}

restart_mitmproxy() {
    docker rm -f "$MITMPROXY_CTR" 2>/dev/null || true
    start_mitmproxy
}

check_proxy_traffic() {
    # Check mitmproxy logs for pypi.org or pythonhosted.org traffic
    local logs
    logs=$(docker logs "$MITMPROXY_CTR" 2>&1)
    if echo "$logs" | grep -qiE 'pypi\.org|pythonhosted\.org|files\.pythonhosted'; then
        return 0
    fi
    echo "  Proxy log snippet:"
    echo "$logs" | tail -20 | sed 's/^/    /'
    return 1
}

# --- Test functions ---

run_basic_test() {
    local backend="$1"
    local port="$2"
    local name
    name="$(safe_name "$backend")"
    local tag="rise-e2e-test-${name}:latest"
    local ctr="rise-e2e-test-${name}"
    local test_label="${backend} - basic build"

    IMAGES_TO_CLEAN+=("$tag")
    CONTAINERS_TO_CLEAN+=("$ctr")

    echo ""
    echo "--- Basic test: $backend ---"

    # Build flags per backend
    local -a flags=()
    case "$backend" in
        docker:buildctl)
            flags+=(--managed-buildkit)
            ;;
        pack)
            flags+=(--builder paketobuildpacks/builder-jammy-base -b paketo-buildpacks/python)
            ;;
        railpack:buildctl)
            flags+=(--managed-buildkit)
            ;;
    esac

    if ! rise_build "$tag" "$FIXTURE_DIR" --backend "$backend" "${flags[@]}"; then
        log_fail "$test_label (build failed)"
        return
    fi

    # Run container
    docker rm -f "$ctr" 2>/dev/null || true
    docker run -d --name "$ctr" -p "${port}:8000" "$tag" >/dev/null

    # Verify HTTP response
    if wait_for_http "http://localhost:${port}/" "rise-e2e-ok" 30; then
        log_pass "$test_label"
    else
        log_fail "$test_label (HTTP check failed)"
        echo "  Container logs:"
        docker logs "$ctr" 2>&1 | tail -10 | sed 's/^/    /'
    fi

    docker rm -f "$ctr" 2>/dev/null || true
}

run_proxy_test() {
    local backend="$1"
    local port="$2"
    local name
    name="$(safe_name "$backend")"
    local tag="rise-e2e-proxy-${name}:latest"
    local ctr="rise-e2e-proxy-${name}"
    local test_label="${backend} - proxy build"

    IMAGES_TO_CLEAN+=("$tag")
    CONTAINERS_TO_CLEAN+=("$ctr")

    echo ""
    echo "--- Proxy test: $backend ---"

    # Restart mitmproxy for clean logs
    restart_mitmproxy

    local ca_bundle
    ca_bundle=$(get_combined_ca_bundle)

    # Determine the host IP that containers can use to reach host port 8080
    local proxy_host
    proxy_host=$(docker network inspect bridge --format '{{(index .IPAM.Config 0).Gateway}}' 2>/dev/null || echo "172.17.0.1")

    # Build flags per backend (all proxy builds use --no-cache)
    local -a flags=(--no-cache)
    case "$backend" in
        docker:buildx)
            flags+=(--managed-buildkit)
            ;;
        docker:buildctl)
            flags+=(--managed-buildkit)
            ;;
        railpack:buildx)
            flags+=(--managed-buildkit)
            ;;
        railpack:buildctl)
            flags+=(--managed-buildkit)
            ;;
    esac

    if ! HTTP_PROXY="http://${proxy_host}:8080" \
         HTTPS_PROXY="http://${proxy_host}:8080" \
         SSL_CERT_FILE="$ca_bundle" \
         rise_build "$tag" "$FIXTURE_DIR" --backend "$backend" "${flags[@]}"; then
        log_fail "$test_label (build failed)"
        return
    fi

    # Verify proxy traffic
    if check_proxy_traffic; then
        log_pass "$test_label"
    else
        log_fail "$test_label (no proxy traffic detected)"
    fi
}

# === Main ===

echo "=== Rise Build E2E Tests ==="
echo "Backends: ${REQUESTED_BACKENDS[*]}"
echo "Proxy tests: $(if $NO_PROXY; then echo disabled; else echo enabled; fi)"
echo ""

port_offset=0
for backend in "${REQUESTED_BACKENDS[@]}"; do
    basic_port=$((BASE_PORT + port_offset))
    proxy_port=$((BASE_PORT + port_offset + 1))
    port_offset=$((port_offset + 2))

    run_basic_test "$backend" "$basic_port"

    if ! $NO_PROXY; then
        run_proxy_test "$backend" "$proxy_port"
    fi
done

# Summary
echo ""
echo "========================================="
echo "  PASSED: $PASSED    FAILED: $FAILED"
echo "========================================="

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi
