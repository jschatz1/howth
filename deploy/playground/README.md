# Howth Playground Deployment

Kubernetes manifests for deploying the Howth playground to `run.howth.run`.

## Prerequisites

- kubectl configured for your cluster
- nginx ingress controller
- cert-manager with `letsencrypt-prod` ClusterIssuer

## Deploy

```bash
# Apply all manifests
kubectl apply -k deploy/playground/

# Or apply individually
kubectl apply -f deploy/playground/namespace.yaml
kubectl apply -f deploy/playground/deployment.yaml
kubectl apply -f deploy/playground/service.yaml
kubectl apply -f deploy/playground/ingress.yaml
```

## Update

```bash
# Restart pods to pull latest image
kubectl rollout restart deployment/playground -n howth-playground

# Or set a specific image tag
kubectl set image deployment/playground playground=ghcr.io/jschatz1/howth:v1.0.0 -n howth-playground
```

## Check status

```bash
kubectl get pods -n howth-playground
kubectl get ingress -n howth-playground
kubectl logs -l app=playground -n howth-playground
```

## Delete

```bash
kubectl delete -k deploy/playground/
```
