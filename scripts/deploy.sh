#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/deploy.sh <project> [tag]
# Examples:
#   ./scripts/deploy.sh frontend
#   ./scripts/deploy.sh ai-service abcd123

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$1"
TAG="${2:-}"

if [ -z "$PROJECT" ]; then
  echo "Usage: $0 <project> [tag]" >&2
  exit 2
fi

cd "$REPO_ROOT"
if [ -z "$TAG" ]; then
  TAG=$(git rev-parse --short HEAD || date +%s)
fi

echo "Deploying project=$PROJECT tag=$TAG"

case "$PROJECT" in
  frontend)
    IMAGE_NAME="world-news-frontend"
    BUILD_CONTEXT="frontend"
    DOCKERFILE="$REPO_ROOT/frontend/Dockerfile"
    K8S_NAME="frontend"
    ;;
  ai-service|ai_service)
    IMAGE_NAME="world-news-ai-service"
    BUILD_CONTEXT="backend/ai-service"
    DOCKERFILE="$REPO_ROOT/backend/ai-service/Dockerfile"
    K8S_NAME="ai-service"
    ;;
  api-gateway|api_gateway)
    IMAGE_NAME="world-news-api-gateway"
    BUILD_CONTEXT="backend/api-gateway"
    DOCKERFILE="$REPO_ROOT/backend/api-gateway/Dockerfile"
    K8S_NAME="api-gateway"
    ;;
  scraper)
    IMAGE_NAME="world-news-scraper"
    BUILD_CONTEXT="backend/scraper"
    DOCKERFILE="$REPO_ROOT/backend/scraper/Dockerfile"
    K8S_NAME="scraper"
    ;;
  *)
    echo "Unknown project: $PROJECT" >&2
    exit 3
    ;;
esac

echo "Building Docker image ${IMAGE_NAME}:${TAG} from ${BUILD_CONTEXT}"
if [ ! -f "$DOCKERFILE" ]; then
  echo "Dockerfile not found at $DOCKERFILE" >&2
  exit 4
fi

docker build -t "${IMAGE_NAME}:${TAG}" -f "$DOCKERFILE" "$REPO_ROOT/$BUILD_CONTEXT"

# Try to make the image available to k3s. k3s often runs containerd; the easiest
# approach is to import the image into the k3s containerd instance. If your k3s
# is configured to use Docker, the image will already be available.

if command -v k3s >/dev/null 2>&1; then
  echo "Detected k3s on host — importing image into k3s containerd"
  # Use sudo where necessary (k3s typically requires root)
  docker save "${IMAGE_NAME}:${TAG}" | sudo k3s ctr images import -
else
  echo "k3s command not found. If your cluster does not see Docker images,
please configure a registry or run this script on the k3s host where ctr is available."
fi

echo "Updating Kubernetes deployment '${K8S_NAME}' to image ${IMAGE_NAME}:${TAG}"
if kubectl -n default set image deployment/${K8S_NAME} ${K8S_NAME}=${IMAGE_NAME}:${TAG} --record; then
  echo "Deployment updated via kubectl set image"
else
  echo "kubectl set image failed — trying to apply k8s manifest at k8s/${PROJECT}-deployment.yaml if present"
  if [ -f "$REPO_ROOT/k8s/${PROJECT}-deployment.yaml" ]; then
    kubectl apply -f "$REPO_ROOT/k8s/${PROJECT}-deployment.yaml"
  else
    echo "No manifest found at k8s/${PROJECT}-deployment.yaml — please create one or update your cluster manually." >&2
    exit 5
  fi
fi

echo "Deployment finished for ${PROJECT}:${TAG}"
