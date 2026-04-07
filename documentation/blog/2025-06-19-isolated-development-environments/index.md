---
title: "Isolated Dev Environments in Goose with container-use"
description: Never worry about breaking your development setup again with containerized, git-branch-isolated development environments powered by container-use
authors:
    - mic
---

![blog cover](sandbox.png)

Over ten years ago, Docker came onto the scene and introduced developers en masse to the concept and practice of containers. These containers helped solve deployment and build-time problems, and in some cases, issues with development environments. They quickly became mainstream. The technology underlying containers included copy-on-write filesystems and lightweight, virtual-machine-like environments that helped isolate processes and simplify cleanup.

Dagger, the project and company founded by Docker’s creator [Solomon Hykes](https://www.linkedin.com/in/solomonhykes), has furthered the reach of containers for developers.

 One project that emerged from this work is [Container Use](https://github.com/dagger/container-use), an MCP server that gives agents an interface for working in isolated containers and git branches. It supports clear lifecycles, easy rollbacks, and safer experimentation, without sacrificing the ergonomics developers expect from local agents.

Container Use brings containerized, git-branch-isolated development directly into your [Goose](/) workflow. While still early in its development, it's evolving quickly and already offers helpful tools for lightweight, branch-specific isolation when you need it.

<!-- truncate -->

## The Problem with Local-Only Development

Traditionally, developers build directly on their local machines, but that approach carries risks such as:

- Dependencies can conflict between projects
- System changes might break other tools
- Experimental code risks your stable codebase
- Cleanup after failed experiments is tedious
- Processes are left running, resources consumed that aren't freed
- Changes are made which can't easily be undone

## A Safer Alternative: Isolated Development Environments

Container Use solves these problems by giving Goose the ability to work in completely isolated environments. Every experiment gets its own sandbox where nothing can affect your main development setup.

- **Git branch isolation**:  Each experiment automatically gets its own git branch, keeping code changes separate from your main codebase.
- **Container isolation**:  Your code runs in clean, reproducible containers with exactly the dependencies it needs—nothing more, nothing less.
- **Easy reset**: When you're done experimenting, simply exit the environment. No cleanup required, no residual changes to worry about.

## Getting Started

### 1. Install Container Use

**macOS (recommended):**
```bash
brew install dagger/tap/container-use
```

**All platforms:**
```bash
curl -fsSL https://raw.githubusercontent.com/dagger/container-use/main/install.sh | bash
```

### 2. Add to Goose

Click this link to automatically add the extension:

**[🚀 Add Container Use to Goose](goose://extension?cmd=cu&arg=stdio&id=container-use&name=container%20use&description=use%20containers%20with%20dagger%20and%20git%20for%20isolated%20environments)**

Or manually add to `~/.config/goose/config.yaml`:

```yaml
extensions:
  container-use:
    name: container-use
    type: stdio
    enabled: true
    cmd: cu
    args:
    - stdio
    envs: {}
```

## Real-World Use Cases

### Experimenting with New Dependencies

- **Prompt**: "I want to try adding Redis to this project, but I'm not sure if it's the right fit. Can you set up an isolated environment?"

- **Result**: Goose creates a new git branch, spins up a container with Redis, and lets you experiment. If it doesn't work out, simply exit—no cleanup needed.

### Risky Refactors

- **Prompt**: "I want to completely restructure this codebase, but I need to be able to roll back easily."

- **Result**:  Work in an isolated branch and container where you can make sweeping changes without fear. Test your new architecture thoroughly. If the refactor succeeds, merge it back to main. If it fails, delete the branch and container.

### Learning New Technologies

- **Prompt**: "I want to try this new framework without installing dependencies on my main system."

- **Result**: Experiment in a pre-configured container with all the tools you need. Learn at your own pace without cluttering your host system or worrying about version conflicts.

### Split Testing Features

- **Prompt**: "I want to test two different approaches to this feature - one using a REST API and another with GraphQL. Can you run both experiments simultaneously?"

- **Result**: Goose spins up two isolated environments, each with its own git branch and container. One agent works on the REST implementation while another tackles GraphQL, both running in parallel without interfering with each other or your main codebase. Compare results and merge the winner.

## Guide

**[Get started with the full guide →](/docs/tutorials/isolated-development-environments)**

---

*Questions? Join our [GitHub discussions](https://github.com/aaif-goose/goose) or [Discord](https://discord.gg/goose-oss). Learn more about Dagger at [dagger.io](https://dagger.io/).*

{/* Video Player */}
<div style={{ width: '100%', maxWidth: '800px', margin: '0 auto' }}>
  <iframe 
    width="560" 
    height="315" 
    src="https://www.youtube.com/embed/pGce9T4E5Yw?si=1D3Aoa6oiFgJ0E5w" 
    title="YouTube video player" 
    frameBorder="0" 
    allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" 
    referrerPolicy="strict-origin-when-cross-origin" 
    allowFullScreen>
  </iframe>
</div>

<head>
  <meta property="og:title" content="Isolated Dev Environments in Goose with container-use" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/06/19/isolated-development-environments" />
  <meta property="og:description" content="Never worry about breaking your development setup again with containerized, git-branch-isolated development environments powered by container-use" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/sandbox-0b0f5e6f871cbf48ea1a0be243440aa1.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Isolated Dev Environments in Goose with container-use" />
  <meta name="twitter:description" content="Never worry about breaking your development setup again with containerized, git-branch-isolated development environments powered by container-use" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/sandbox-0b0f5e6f871cbf48ea1a0be243440aa1.png" />
</head>