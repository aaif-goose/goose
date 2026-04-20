---
title: "Mesh LLM in goose: routing across models"
description: "Mesh LLM is now available as a provider setting in goose."
authors:
    - mic
---

Quick note: [Mesh LLM](https://github.com/Mesh-LLM/mesh-llm/) is now in goose as an option for accessing and sharing (open) LLMs with friends and family.

It uses the same llama.cpp infra as local mode to run models, with a twist.

<!--truncate-->

## What is Mesh LLM?

Mesh LLM is an associated project we're trying out that lets people connect their compute capacity (which may just be a laptop) peer-to-peer, so they can access models they may not otherwise be able to self-host.

There is a demo public "mesh" which at any point has some capacity in it, but you can also make your own private networks and pool compute together. The mesh will try to work out the best places to run models (downloading them as needed) and can even split the compute in various ways.

This is a pretty early-stage project, so we'd love any feedback on it.

Check out [the project docs](https://docs.anarchai.org/) and the [live public mesh](https://meshllm.cloud/dashboard).

<head>
  <meta property="og:title" content="Mesh LLM in goose: routing across models" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/04/20/mesh-llm" />
  <meta property="og:description" content="Mesh LLM is now available as a provider setting in goose." />
</head>
