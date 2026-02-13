# Local Development

The `rise run` command builds and runs your application locally in a container, simulating a deployment environment for development and testing.

## Basic Usage

```bash
# Build and run from current directory (port 8080)
rise run

# Specify directory
rise run ./path/to/app
```

## Port Configuration

- `--http-port` — the port your application listens on inside the container (also sets the `PORT` env var)
- `--expose` — the port exposed on your host machine (defaults to same as `--http-port`)

```bash
# Application listens on 3000, accessible at http://localhost:3000
rise run --http-port 3000

# Application listens on 8080, accessible at http://localhost:3000
rise run --http-port 8080 --expose 3000
```

## Project Environment Variables

Load non-secret environment variables from a Rise project:

```bash
rise run --project my-app
```

This is enabled by default when `--project` is specified. Secret variables cannot be loaded (their values are not retrievable from the backend). Protected secrets are skipped with a warning.

Disable with `--use-project-env=false`.

## Runtime Environment Overrides

Set or override environment variables for the local run:

```bash
rise run --run-env DATABASE_URL=postgres://localhost/mydb --run-env DEBUG=true
```

`--run-env` values take precedence over project environment variables.

## Build Backend Selection

Use any build backend:

```bash
rise run --backend pack
rise run --backend railpack
rise run --backend docker --dockerfile Dockerfile.dev
```

All standard [build flags](builds.md) are supported.

## Standalone Image Build

Build an image without running it:

```bash
rise build myapp:latest
rise build myapp:latest --backend pack
```

Push the built image to a registry:

```bash
rise build myapp:latest --push
```

## How It Works

1. Builds the container image using the selected backend
2. Tags the image as `rise-local-{project-name}`
3. Loads non-secret environment variables from the project (if specified)
4. Runs `docker run --rm -it -p {expose}:{http-port}` with the image
5. Sets `PORT` environment variable automatically
6. Container is removed when stopped (`--rm`)

Press Ctrl+C to stop the container.
