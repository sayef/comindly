# Storage

An index lives in a folder or in a bucket:

| Location | Example |
| --- | --- |
| Folder (local disk, a mounted volume) | `~/comindly-index`, `/data/index` |
| Amazon S3 and S3-compatible stores (MinIO, R2, Ceph) | `s3://bucket/comindly` |
| Google Cloud Storage | `gs://bucket/comindly` |
| Azure Blob Storage | `az://container/comindly` |

comindly has no storage settings or sign-in of its own. The engine reads a bucket with the credentials
the machine or container already has, the same way that platform's own tools do. Give comindly access
where the engine runs; there is nothing to configure in comindly. The local engine gets the environment
of the command that starts it, so start it from a shell with the credentials (`comindly engine
start`), or stop it and run the next command from such a shell.

## Amazon S3

The standard AWS chain: `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`, `AWS_PROFILE` (including SSO
profiles after `aws sso login`), web identity (OIDC in CI, EKS service accounts), and ECS task or EC2
instance roles. Region comes from `AWS_REGION` or the profile.

## S3-compatible stores

Point comindly at the store's endpoint:

```sh
export AWS_ENDPOINT_URL=http://localhost:9000
export AWS_ALLOW_HTTP=true             # plain http, for a local store only
export AWS_ACCESS_KEY_ID=… AWS_SECRET_ACCESS_KEY=… AWS_REGION=us-east-1
comindly engine stop                    # so the next command starts it with these
comindly config set index_dir=s3://comindly/org
comindly link ../repo-a ../repo-b
```

## Google Cloud Storage

Application default credentials: `GOOGLE_APPLICATION_CREDENTIALS` (a service-account file), the
credentials from `gcloud auth application-default login`, or the workload's service account on GKE,
Cloud Run and Compute Engine.

## Azure Blob Storage

Set `AZURE_STORAGE_ACCOUNT_NAME`, plus `AZURE_STORAGE_ACCOUNT_KEY` or a SAS token, or run with a managed
identity or workload identity on AKS and Azure Container Apps.

## Problems

`comindly doctor` and the app say when an index cannot be read, with the storage service's reason
(expired or rejected credentials, access denied, a missing bucket) and never the credentials themselves.
