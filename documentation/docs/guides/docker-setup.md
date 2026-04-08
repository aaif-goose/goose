---
title: Docker Setup Guide
sidebar_label: Docker Setup
description: Step-by-step guide to install and run goose using Docker
---

# Docker Setup Guide

This guide walks you through setting up and running goose using Docker. Whether you want to run goose itself inside a container or run extensions in existing containers, this guide covers both scenarios.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) installed (version 20.10 or later)
- [Docker Compose](https://docs.docker.com/compose/install/) installed
- An API key for your preferred LLM provider

## Quick Start with Pre-built Images

The fastest way to get started is using the pre-built image from GitHub Container Registry:

```bash
# Pull the latest image
docker pull ghcr.io/aaif-goose/goose:latest

# Verify the installation
docker run --rm ghcr.io/aaif-goose/goose:latest --version

# Run goose with your LLM provider
docker run --rm -it \
  -e GOOSE_PROVIDER=openai \
  -e GOOSE_MODEL=gpt-4o \
  -e OPENAI_API_KEY=$OPENAI_API_KEY \
  ghcr.io/aaif-goose/goose:latest session
```

## Building and Running from Source

If you need to build goose from source within a Docker container, this approach provides an isolated and reproducible environment. It's especially useful for debugging platform-specific issues (e.g., testing on Ubuntu when you normally work on macOS).

### Step 1: Configure Docker Compose

Modify the [`Dockerfile` and `docker-compose.yml` files](https://github.com/aaif-goose/goose/tree/main/documentation/docs/docker) to suit your requirements:

- **Required:** Set your API key, provider, and model in `docker-compose.yml` as environment variables. The keyring does not work inside Docker containers.

  ```yaml
  environment:
    - GOOGLE_API_KEY=your-api-key-here
    - GOOSE_PROVIDER=google
    - GOOSE_MODEL=gemini-2.0-flash-exp
  ```

- **Optional:** Change the base image in the `Dockerfile` to a different Linux distribution (default is Ubuntu, but you can use CentOS, Fedora, or Alpine).

- **Optional:** Mount your personal goose settings and hints files in `docker-compose.yml` to use your existing configuration inside the container.

:::tip Automated Alternative
For an automated approach to running goose in containers, see the [Container-Use MCP extension](/docs/mcp/container-use-mcp), which creates and manages containers for you through conversation.
:::

### Step 2: Build the Docker Image

```bash
docker-compose -f documentation/docs/docker/docker-compose.yml build
```

### Step 3: Run the Container

```bash
docker-compose -f documentation/docs/docker/docker-compose.yml run --rm goose-cli
```

### Step 4: Configure goose Inside the Container

```bash
goose configure
```

When prompted to save the API key to the keyring, select **No** since the API key is already set as an environment variable.

Run `goose configure` a second time to [add any extensions](/docs/getting-started/using-extensions) you need.

### Step 5: Start a Session

```bash
goose session
```

You should now be connected to goose with your configured extensions enabled.

## Running Extensions in Docker Containers

The `--container` flag lets you run goose on your host machine while executing extensions inside a Docker container.

### Basic Usage

```bash
goose session --container <container-id-or-name>
```

Extensions configured in your `config.yaml` will automatically run inside the specified container. Find your container ID or name with `docker ps`.

### Requirements

- Extensions must exist in the container and be accessible via the same paths used in your extension config
- To run built-in extensions, the goose CLI must be [installed](/docs/getting-started/installation) inside the container

### Examples

```bash
# Start an interactive session with extensions from config.yaml
goose session --container my-dev-container

# Start a non-interactive session with instructions
goose run --container my-dev-container --text "your instructions here"

# Specify an extension to run in the container
goose session --container 4c76a1beed85 --with-extension "uvx mcp-server-fetch"

# Workaround: Use full path if container can't find the command
goose session --container 4c76a1beed85 --with-extension "/root/.local/bin/uvx mcp-server-fetch"
```

## Troubleshooting

### Keyring Errors

If you see keyring-related errors, make sure you're passing your API key as an environment variable and selecting **No** when prompted to save to the keyring during `goose configure`.

### Extension Not Found in Container

If an extension command isn't found, try using the full path to the binary:

```bash
goose session --container my-container --with-extension "/full/path/to/extension"
```

### Build Failures

If the Docker build fails, ensure you have enough disk space and memory allocated to Docker. The build process compiles Rust code, which can be resource-intensive.
