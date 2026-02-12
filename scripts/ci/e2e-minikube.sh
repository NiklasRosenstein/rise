#!/usr/bin/env bash
set -euo pipefail

NAMESPACE="${NAMESPACE:-rise-ci}"
RELEASE_NAME="${RELEASE_NAME:-rise-ci}"
IMAGE_REPOSITORY="${RISE_IMAGE_REPOSITORY:?RISE_IMAGE_REPOSITORY is required}"
IMAGE_TAG="${RISE_IMAGE_TAG:?RISE_IMAGE_TAG is required}"

cleanup() {
  local exit_code=$?
  if [[ -n "${PF_PID:-}" ]] && kill -0 "${PF_PID}" >/dev/null 2>&1; then
    kill "${PF_PID}" >/dev/null 2>&1 || true
  fi
  if [[ $exit_code -ne 0 ]]; then
    kubectl get pods -A || true
    kubectl get events -A --sort-by=.metadata.creationTimestamp | tail -n 200 || true
  fi
}
trap cleanup EXIT

echo "Starting Minikube"
minikube start --driver=docker --cpus=2 --memory=4096
minikube addons enable ingress

echo "Installing chart with CI image ${IMAGE_REPOSITORY}:${IMAGE_TAG}"
echo "Using CI values from helm/rise/values-ci.yaml"
cat helm/rise/values-ci.yaml

helm upgrade --install "${RELEASE_NAME}" ./helm/rise \
  --namespace "${NAMESPACE}" \
  --create-namespace \
  --values helm/rise/values-ci.yaml \
  --set "image.repository=${IMAGE_REPOSITORY}" \
  --set "image.tag=${IMAGE_TAG}" \
  --set "image.pullPolicy=Always" \
  --set-string "config.deployment_controller.auth_backend_url=http://${RELEASE_NAME}-server.${NAMESPACE}.svc.cluster.local:3000" \
  --set-string "config.deployment_controller.auth_signin_url=http://rise.local"

echo "Waiting for workloads to become ready"
kubectl wait --namespace "${NAMESPACE}" --for=condition=Available deployment -l "app.kubernetes.io/instance=${RELEASE_NAME}" --timeout=10m
kubectl wait --namespace "${NAMESPACE}" --for=condition=Ready pod -l "app.kubernetes.io/instance=${RELEASE_NAME}" --timeout=10m

server_service="$(kubectl get svc -n "${NAMESPACE}" -l "app.kubernetes.io/instance=${RELEASE_NAME},app.kubernetes.io/component=server" -o jsonpath='{.items[0].metadata.name}')"
if [[ -z "${server_service}" ]]; then
  echo "Failed to locate server service"
  exit 1
fi

echo "Port-forwarding ${server_service}"
kubectl -n "${NAMESPACE}" port-forward "svc/${server_service}" 3000:3000 >/tmp/rise-e2e-port-forward.log 2>&1 &
PF_PID=$!
sleep 5

echo "Smoke test: /health endpoint"
curl --fail --silent --show-error "http://127.0.0.1:3000/health" | grep -qi "ok"

echo "Smoke test: protected API returns auth error"
http_code="$(curl --silent --show-error --output /dev/null --write-out "%{http_code}" "http://127.0.0.1:3000/api/v1/projects")"
if [[ "${http_code}" != "401" && "${http_code}" != "403" ]]; then
  echo "Expected 401/403 for unauthenticated request, got ${http_code}"
  exit 1
fi

echo "Smoke test: helm upgrade is idempotent"
helm upgrade "${RELEASE_NAME}" ./helm/rise \
  --namespace "${NAMESPACE}" \
  --values helm/rise/values-ci.yaml \
  --set "image.repository=${IMAGE_REPOSITORY}" \
  --set "image.tag=${IMAGE_TAG}" \
  --set "image.pullPolicy=Always" \
  --set-string "config.deployment_controller.auth_backend_url=http://${RELEASE_NAME}-server.${NAMESPACE}.svc.cluster.local:3000" \
  --set-string "config.deployment_controller.auth_signin_url=http://rise.local"

echo "Minikube E2E smoke tests completed successfully"
