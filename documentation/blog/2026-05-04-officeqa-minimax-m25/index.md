---
title: "Approaching frontier performance for 1/30th the cost"
description: "Using goose with MiniMax M2.5 on the Databricks OfficeQA benchmark"
authors:
    - mic
---

Quick highlight: [Sentient AGI](https://x.com/SentientAGI/status/2046967422004154739) recently shared with us some results with a challenge called "Grounded Reasoning" where teams tackled the Databricks OfficeQA benchmark using the MiniMax (2.5) open model.

Highlights from the linked article:

> * Using goose with [MiniMax M2.5](https://www.minimax.io/) on the [OfficeQA benchmark from Databricks](https://www.databricks.com/blog/officeqa), we're seeing results approaching frontier model performance for **1/30th of the cost**.
> * Closed-source wins on accuracy (~80% vs ~70%), but MiniMax M2.5 ran at an average of $1.74 per run versus $56.53 for Opus 4.5. That's roughly 30× cheaper.
> * Switching to the @goose_oss harness beat alternatives by ~10% in accuracy and was 8× cheaper than the next option. We decided to further investigate this with independent testing. On Terminal Bench 2.0, we found a similar story—goose was 20x more token efficient (and cheaper) than OpenHands, and more than 40x cheaper than OpenCode.

Check out the full post and results here:

> **[https://x.com/SentientAGI/status/2046967422004154739](https://x.com/SentientAGI/status/2046967422004154739)**

<head>
  <meta property="og:title" content="Approaching frontier performance for 1/30th the cost" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2026/05/04/officeqa-minimax-m25" />
  <meta property="og:description" content="Using goose with MiniMax M2.5 on the Databricks OfficeQA benchmark — approaching frontier model performance at a fraction of the cost." />
</head>
