---
title: Rendex Extension
description: Add Rendex MCP Server as a goose Extension for Screenshots, PDFs, and HTML-to-Image Rendering
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

This tutorial covers how to add the [Rendex MCP Server](https://github.com/copperline-labs/rendex-mcp) as a goose extension to capture screenshots, generate PDFs, and render HTML to images from any webpage or raw HTML — useful for archiving UIs, generating invoices and reports, producing OG images, and giving goose a reliable "see the web" capability that doesn't require spinning up a full browser automation stack.

:::tip Quick Install

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=-y&arg=@copperline/rendex-mcp&id=rendex&name=Rendex&description=Capture%20screenshots%2C%20generate%20PDFs%2C%20and%20render%20HTML%20to%20images%20via%20AI%20agents&env=RENDEX_API_KEY%3DRendex%20API%20Key)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx -y @copperline/rendex-mcp
  ```
  </TabItem>
</Tabs>
  **Environment Variable**
  ```
  RENDEX_API_KEY: <YOUR_API_KEY>
  ```
:::

## Configuration

:::info
Note that you'll need [Node.js](https://nodejs.org/) installed on your system to run this command, as it uses `npx`.
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  <GooseDesktopInstaller
    extensionId="rendex"
    extensionName="Rendex"
    description="Capture screenshots, generate PDFs, and render HTML to images via AI agents"
    command="npx"
    args={["-y", "@copperline/rendex-mcp"]}
    envVars={[
      { name: "RENDEX_API_KEY", label: "Rendex API Key" }
    ]}
    apiKeyLink="https://rendex.dev/dashboard/keys"
    apiKeyLinkText="Rendex API key"
  />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Rendex"
      description="Capture screenshots, generate PDFs, and render HTML to images via AI agents"
      command="npx -y @copperline/rendex-mcp"
      envVars={[
        { key: "RENDEX_API_KEY", value: "▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪" }
      ]}
      infoNote={
        <>
          Get your API key from{" "}
          <a href="https://rendex.dev/dashboard/keys" target="_blank" rel="noopener noreferrer">
            rendex.dev
          </a> and paste it in. Free tier includes 500 calls/month.
        </>
      }
    />
  </TabItem>
</Tabs>

## Available Tool

Rendex exposes a single outcome-focused tool, `rendex_screenshot`, that handles screenshots, PDF generation, and HTML-to-image rendering with typed parameters for every option.

| Capability | Notes |
|---|---|
| **Output formats** | PNG, JPEG, WebP, PDF |
| **Full-page capture** | Scroll-and-stitch, with `bestAttempt` fallback on heavy sites |
| **Dark mode** | Emulates `prefers-color-scheme: dark` |
| **Element selector** | Capture a specific element (`#hero`, `.pricing-card`) instead of the whole viewport |
| **CSS/JS injection** | Inject up to 50KB of custom CSS/JS before capture — hide cookie banners, add watermarks, override styles |
| **Cookie/header injection** | Set up to 50 cookies and arbitrary headers for authenticated pages |
| **PDF options** | Page size (A4/Letter/Legal/Tabloid/A3), landscape, margins, scale, print background |
| **Geo-targeting** | ISO country code + city/state for country-specific rendering (Pro/Enterprise) |
| **Async pipeline** | Set `async=true` to get a job ID + HMAC-signed webhook callback when the render completes |
| **Caching** | `cacheTtl` returns a signed R2 URL for cached results |
| **Raw HTML input** | Pass `html` instead of `url` to render arbitrary markup (invoices, social cards, email templates, OG images) |

Rendex runs on Cloudflare Workers with Browser Rendering on the back end, so Node/Chromium doesn't have to spin up locally.

## Example Usage

Let's use the Rendex extension to generate an invoice PDF from raw HTML and archive a product page.

### goose Prompt

```
1. Render this HTML as an A4 PDF with 1cm margins:

   <!doctype html>
   <html>
     <body style="font-family: system-ui; padding: 40px;">
       <h1>Invoice #0042</h1>
       <p>Bill to: Acme Corp</p>
       <table>
         <tr><td>MCP server listing</td><td>$500.00</td></tr>
         <tr><td>Total</td><td><b>$500.00</b></td></tr>
       </table>
     </body>
   </html>

2. Then take a full-page screenshot of https://news.ycombinator.com in dark mode

3. Return both results so I can save them
```

### goose Output

```
I'll generate the invoice PDF first, then capture the HN page.

[rendex_screenshot: format=pdf, pdfFormat=A4, pdfMargin={top: 1cm, ...}, html=<invoice markup>]
✓ PDF captured successfully (12,344 bytes)
  width: 1280  height: 800  capturedAt: 2026-04-15T12:34:56Z

[rendex_screenshot: url=https://news.ycombinator.com, fullPage=true, darkMode=true]
✓ Screenshot captured (PNG, 847KB)
  width: 1280  height: 4,321  waitStrategy: networkidle2

Both renders completed. You have:
  1. Invoice PDF (A4, 12.3KB) — base64 in result 1
  2. Dark-mode full-page screenshot of Hacker News (PNG, 847KB) — base64 in result 2
```

## Pricing

Rendex is free to try — you don't need a credit card for the free tier.

| Plan | Calls/Month | Rate limit |
|---|---|---|
| Free | 500 | 10/min |
| Starter | 10,000 | 60/min |
| Pro | 100,000 | 300/min |
| Enterprise | Custom | 1,000/min |

Get an API key at [rendex.dev](https://rendex.dev).

## Remote (Zero-Install) Option

Rendex also exposes a remote Streamable HTTP endpoint at `https://mcp.rendex.dev/mcp`. If your goose client supports the `type: http` transport, you can skip the `npx` step entirely:

```yaml
extensions:
  rendex:
    type: http
    url: https://mcp.rendex.dev/mcp
    envVars:
      RENDEX_API_KEY: your-api-key  # sent as Authorization: Bearer <key>
```

The remote endpoint uses Bearer auth — goose will pass `RENDEX_API_KEY` as `Authorization: Bearer <key>` to every request.

## Links

- **Website**: [rendex.dev](https://rendex.dev)
- **GitHub**: [copperline-labs/rendex-mcp](https://github.com/copperline-labs/rendex-mcp)
- **npm**: [`@copperline/rendex-mcp`](https://www.npmjs.com/package/@copperline/rendex-mcp)
- **Smithery**: [smithery.ai/server/copperline/rendex-mcp](https://smithery.ai/server/copperline/rendex-mcp)
- **Official MCP Registry**: `io.github.copperline-labs/rendex-mcp`
