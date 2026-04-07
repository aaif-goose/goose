---
title: Managing Tools
hide_title: true
description: Control and configure the tools and extensions that power your goose workflows
---

import Card from '@site/src/components/Card';
import styles from '@site/src/components/Card/styles.module.css';

<h1 className={styles.pageTitle}>Managing Tools</h1>
<p className={styles.pageDescription}>
  Tools are specific functions within <a href="/docs/getting-started/using-extensions">extensions</a> that give goose its capabilities. Learn to control and customize how these tools work for you.
</p>

<div className={styles.categorySection}>
  <h2 className={styles.categoryTitle}>📚 Documentation & Guides</h2>
  <div className={styles.cardGrid}>
    <Card 
      title="Tool Permissions"
      description="Configure fine-grained permissions to control which tools goose can use and when, ensuring secure and controlled automation."
      link="/docs/guides/managing-tools/tool-permissions"
    />
    <Card 
      title="Adjust Tool Output"
      description="Customize how tool interactions are displayed, from detailed verbose output to clean concise summaries."
      link="/docs/guides/managing-tools/adjust-tool-output"
    />
    <Card 
      title="Code Mode"
      description="Programmatic approach that discovers and calls MCP tools on demand."
      link="/docs/guides/managing-tools/code-mode"
    />
    <Card 
      title="Ollama Tool Shim"
      description="Enable tool calling for models that don't natively support it using an experimental local interpreter model setup."
      link="/docs/experimental/ollama"
    />
  </div>
</div>

<div className={styles.categorySection}>
  <h2 className={styles.categoryTitle}>📝 Featured Blog Posts</h2>
  <div className={styles.cardGrid}>
    <Card
      title="Agentic AI and the MCP Ecosystem"
      description="A 101 introduction to AI agents, tool calling, and how tools work with LLMs to enable powerful automation."
      link="/blog/2025/02/17/agentic-ai-mcp"
    />
    <Card
      title="A Visual Guide To MCP Ecosystem"
      description="Visual breakdown of MCP: How your AI agent, tools, and models work together, explained with diagrams and analogies."
      link="/blog/2025/04/10/visual-guide-mcp"
    />
    <Card
      title="Finetuning Toolshim Models for Tool Calling"
      description="Technical deep-dive into the challenges of tool calling with open-source models and the research behind toolshim solutions."
      link="/blog/2025/04/11/finetuning-toolshim"
    />
  </div>
</div>
