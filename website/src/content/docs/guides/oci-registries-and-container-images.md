---
title: OCI Registries and Container Images
description: Publish a Tuff pack to Amazon ECR, pull it by digest, and add its extracted target to a runnable container image.
---

This guide follows one pack from source to Amazon Elastic Container Registry (ECR), then into a runnable container image. The same Tuff commands work with other OCI-compatible registries; only repository provisioning and login differ.

## First: understand the two layers

The word “layer” refers to two different things in this workflow:

| Layer | Contains | Created by | Runnable by Docker? |
| --- | --- | --- | --- |
| Tuff OCI artifact layer | The exact `.tuffpack` bytes | `tuff pack push` | No |
| Container image filesystem layer | Extracted `.agents/...` and other harness-native files | Dockerfile `COPY` | Yes, as part of the image |

A Tuff pack uses an OCI manifest so a registry can store and transport it, but it is not a container image. It has an empty OCI configuration and a Tuff-specific layer media type rather than an operating system, architecture, command, or root filesystem. Do not use `docker pull` or `FROM` with a Tuff pack reference.

The supported deployment flow is:

```text
pack source
  → tuff pack build
  → tuff pack push to ECR
  → immutable manifest-digest reference
  → tuff pack pull
  → tuff pack extract for one agent
  → Docker BuildKit COPY
  → runnable container image
```

## Prerequisites

You need:

- Tuff installed;
- AWS CLI v2 configured for the target account and Region;
- permission to create or use an ECR private repository;
- Docker with Buildx/BuildKit; and
- `jq` if you want to capture Tuff's JSON result automatically in a shell or CI job.

This guide uses account `111122223333`, Region `eu-west-2`, ECR repository `tuff/crm-integration`, and pack version `1.2.0`. Replace them with your values.

```sh frame="terminal"
export AWS_ACCOUNT_ID="111122223333"
export AWS_REGION="eu-west-2"
export PACK_REPOSITORY="tuff/crm-integration"
export PACK_TAG="1.2.0"
export ECR_REGISTRY="${AWS_ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com"
export PACK_TAG_REFERENCE="${ECR_REGISTRY}/${PACK_REPOSITORY}:${PACK_TAG}"
```

OCI references do not include `https://`. Tuff uses HTTPS by default.

## Create an immutable ECR repository

Create the repository once. Skip this command when your platform team already provisions it.

```sh frame="terminal"
aws ecr create-repository \
  --region "$AWS_REGION" \
  --repository-name "$PACK_REPOSITORY" \
  --image-tag-mutability IMMUTABLE
```

ECR tag immutability complements Tuff's safe-tag default. Repeating a push of the same Tuff manifest still reports `unchanged` because Tuff detects that before publishing. A different manifest is refused by Tuff without `--force`, and an immutable ECR repository also rejects an attempted forced tag move. Use a new version tag instead of `--force` for releases.

Amazon ECR private repositories support OCI-compatible artifacts. See the [ECR repository documentation](https://docs.aws.amazon.com/AmazonECR/latest/userguide/Repositories.html) and [`create-repository` reference](https://docs.aws.amazon.com/cli/latest/reference/ecr/create-repository.html).

## Grant the publisher and consumer permissions

The following combined policy covers login, Tuff's safe-tag read, blob publication, and later pull. Replace the account, Region, and repository in `Resource`. Repository creation is intentionally separate and requires `ecr:CreateRepository` for the provisioning identity.

```json title="tuff-pack-ecr-policy.json"
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "AuthenticateToEcr",
      "Effect": "Allow",
      "Action": "ecr:GetAuthorizationToken",
      "Resource": "*"
    },
    {
      "Sid": "PublishAndPullTuffPacks",
      "Effect": "Allow",
      "Action": [
        "ecr:BatchCheckLayerAvailability",
        "ecr:BatchGetImage",
        "ecr:CompleteLayerUpload",
        "ecr:GetDownloadUrlForLayer",
        "ecr:InitiateLayerUpload",
        "ecr:PutImage",
        "ecr:UploadLayerPart"
      ],
      "Resource": "arn:aws:ecr:eu-west-2:111122223333:repository/tuff/crm-integration"
    }
  ]
}
```

For separate roles, the consumer needs `GetAuthorizationToken`, `BatchGetImage`, and `GetDownloadUrlForLayer`; the publisher needs the remaining upload actions plus `BatchGetImage` for Tuff's existing-tag check. AWS documents the standard repository-scoped publisher policy in [IAM permissions for pushing to ECR](https://docs.aws.amazon.com/AmazonECR/latest/userguide/image-push-iam.html).

## Authenticate Tuff to ECR

Request an ECR token and give it to Docker through standard input:

```sh frame="terminal"
aws ecr get-login-password --region "$AWS_REGION" \
  | docker login \
      --username AWS \
      --password-stdin "$ECR_REGISTRY"
```

`docker login` stores the credential in Docker's configured credential store or helper. Tuff reads that existing Docker configuration, so there is no separate `tuff login` command and the token is not included in the pack. ECR authorization tokens expire; authenticate again at the start of each publisher or consumer job. See AWS's [`get-login-password` example](https://docs.aws.amazon.com/cli/latest/userguide/cli_ecr_code_examples.html#cli_ecr_code_examples_get-login-password).

## Build and publish the pack

Build and locally verify the deterministic artifact:

```sh frame="terminal"
mkdir -p dist
tuff pack check ./crm-integration
tuff pack build ./crm-integration --output dist/crm-integration-1.2.0.tuffpack
tuff pack verify dist/crm-integration-1.2.0.tuffpack
```

Publish it under the explicit ECR tag:

```sh frame="terminal"
tuff pack push \
  dist/crm-integration-1.2.0.tuffpack \
  "$PACK_TAG_REFERENCE" \
  --json
```

The result contains both digests and an immutable reference:

```json
{
  "status": "pushed",
  "name": "crm-integration",
  "version": "1.2.0",
  "artifactDigest": "sha256:<tuffpack-digest>",
  "manifestDigest": "sha256:<oci-manifest-digest>",
  "tagReference": "111122223333.dkr.ecr.eu-west-2.amazonaws.com/tuff/crm-integration:1.2.0",
  "reference": "111122223333.dkr.ecr.eu-west-2.amazonaws.com/tuff/crm-integration@sha256:<oci-manifest-digest>"
}
```

The `reference` field is the release identity to pass to deployment jobs. A tag is readable but mutable in the general OCI model; the manifest digest identifies the exact registry object that was reviewed.

To capture it in CI:

```sh frame="terminal"
mkdir -p build
tuff pack push \
  dist/crm-integration-1.2.0.tuffpack \
  "$PACK_TAG_REFERENCE" \
  --json > build/tuff-pack-push.json

export PACK_DIGEST_REFERENCE="$(jq -r '.reference' build/tuff-pack-push.json)"
printf '%s\n' "$PACK_DIGEST_REFERENCE"
```

Persist `PACK_DIGEST_REFERENCE` as deployment metadata or a downstream CI output. Do not reconstruct it from the tag later.

## Pull and extract the runtime target

The consumer authenticates to ECR in the same way, receives the digest reference from the publisher, and writes to fresh output paths:

```sh frame="terminal"
aws ecr get-login-password --region "$AWS_REGION" \
  | docker login \
      --username AWS \
      --password-stdin "$ECR_REGISTRY"

mkdir -p build
tuff pack pull \
  "$PACK_DIGEST_REFERENCE" \
  --output build/crm-integration-1.2.0.tuffpack

tuff pack extract \
  build/crm-integration-1.2.0.tuffpack \
  --agent open-agents \
  --output build/tuff-runtime
```

`pack pull` verifies the OCI manifest, layer size and digest, complete Tuff artifact, stored files, and metadata annotations before it creates the output file. `pack extract` verifies the artifact again and writes the pre-rendered `open-agents` target. Both commands refuse to overwrite existing output, so CI should use a fresh workspace or new paths.

For `open-agents`, the extracted root contains paths such as `.agents/skills/...`, `.agents/tools/...`, and any shared harness configuration emitted by the adapter. Other `--agent` values produce that adapter's native layout.

## Add the extracted target to a container image

Pass the extracted directory as a named BuildKit context. This keeps ECR authentication and Tuff outside the Docker build and prevents the raw `.tuffpack` from becoming runtime baggage.

```dockerfile title="Dockerfile"
# syntax=docker/dockerfile:1
FROM debian:bookworm-slim

WORKDIR /workspace

# Copies the verified harness-native tree, including hidden .agents paths.
COPY --from=tuff-runtime / /workspace/

# Install the application or agent runtime after this line.
CMD ["sh"]
```

Build the image and load it into the local Docker image store:

```sh frame="terminal"
docker buildx build \
  --build-context tuff-runtime=./build/tuff-runtime \
  --tag example-agent:1.2.0 \
  --load \
  .
```

Docker treats the `COPY` result as ordinary image filesystem content. The image now contains `/workspace/.agents/...`; it does not contain registry credentials unless some unrelated Dockerfile instruction adds them.

For a base image that contains `find`, inspect the result with:

```sh frame="terminal"
docker run --rm --entrypoint find example-agent:1.2.0 \
  /workspace/.agents -maxdepth 4 -type f
```

BuildKit documents local named contexts and `COPY --from=<context>` in [Build context](https://docs.docker.com/build/concepts/context/) and the [`buildx build` reference](https://docs.docker.com/reference/cli/docker/buildx/build/#build-context).

## Why not pull inside the Dockerfile?

Pulling before `docker build` keeps cloud login, registry tokens, Tuff, and provider CLIs out of the build definition and image history. It also creates a clear verification boundary: only an extracted target derived from the approved digest enters the build context.

An advanced BuildKit build could mount secrets and run Tuff inside a build stage, but that adds token handling, networking, tool installation, and cache behavior without improving the resulting image. Tuff standardizes on pre-pull and pre-extract for this workflow.

Copying only the raw `.tuffpack` into an image is also possible, but the runtime would then need Tuff and an extraction step during startup. That delays verification and mutation until runtime, so it is not the recommended immutable-image workflow.

## Other compatible registries

After login, the Tuff build, push, pull, digest, extract, and Docker steps remain unchanged.

| Registry | Configure reusable credentials | Reference shape |
| --- | --- | --- |
| GitHub Container Registry | `printf '%s' "$GHCR_TOKEN" \| docker login ghcr.io --username "$GITHUB_USER" --password-stdin` | `ghcr.io/yourorg/crm-integration:1.2.0` |
| Google Artifact Registry | `gcloud auth configure-docker europe-west2-docker.pkg.dev` | `europe-west2-docker.pkg.dev/project/repository/crm-integration:1.2.0` |
| Azure Container Registry | `az acr login --name myregistry` | `myregistry.azurecr.io/team/crm-integration:1.2.0` |
| Self-hosted OCI registry | `docker login registry.example.com` | `registry.example.com/team/crm-integration:1.2.0` |

Tuff checks Docker credentials first and Podman credentials second. For a self-hosted registry with a private certificate authority, the login client must trust that CA according to its own configuration, and each Tuff push or pull must receive `--ca-file company-ca.pem`; Tuff's flag does not change Docker or Podman trust settings. Use `--plain-http` only with an isolated disposable development registry.

## Common failures

| Error | Likely cause | Resolution |
| --- | --- | --- |
| Authentication or `401 Unauthorized` | Missing, expired, or wrong-Region ECR login | Run `get-login-password` again with the repository's Region and correct registry hostname. |
| Repository or manifest not found | Repository was not provisioned, or reference uses the wrong account, Region, repository, tag, or digest | Check the ECR repository and use the exact reference printed by Tuff. |
| `refusing to move existing OCI tag` | The tag already identifies different content | Publish a new release tag; do not force an immutable ECR tag. |
| `refusing to overwrite` | Pull or extract output already exists | Use a fresh CI workspace or choose a new output path. |
| Docker cannot use the Tuff reference in `FROM` | A Tuff pack is a generic OCI artifact, not a runnable image | Pull with Tuff, extract a target, and pass it as a local named build context. |
