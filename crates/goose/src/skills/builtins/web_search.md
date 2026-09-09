---
name: web-search
description: Search the web and extract page content using DuckDuckGo (no API key required), Tavily, or SearXNG. Use whenever the task needs current information, facts not in training data, or content from a specific URL.
---

## Requirements

- `uv` must be installed (`curl -LsSf https://astral.sh/uv/install.sh | sh`)
- No API key required for the default DuckDuckGo path

## Search

**Default — DuckDuckGo (no API key):**
```bash
uvx ddgs text -q "your query here" -m 5
```

**Tavily (richer results, requires `TAVILY_API_KEY`):**
```bash
uvx --from tavily-python python -c "
import os
from tavily import TavilyClient
r = TavilyClient(os.environ['TAVILY_API_KEY']).search('your query here', max_results=5)
for res in r['results']:
    print(res['url'])
    print(res['content'])
    print()
"
```

**SearXNG (self-hosted, requires `SEARXNG_URL`):**
```bash
curl -sG --data-urlencode "q=your query here" --data "format=json" "${SEARXNG_URL}/search" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for r in data.get('results', [])[:5]:
    print(r['url'])
    print(r.get('content',''))
    print()
"
```

Pick the first available: Tavily if `TAVILY_API_KEY` is set, SearXNG if `SEARXNG_URL` is set, otherwise DuckDuckGo.

## Extract page content

```bash
url="https://example.com"
tmpfile=$(mktemp /tmp/page-XXXXXX)
curl -sL --max-time 15 -A "Mozilla/5.0" "$url" | uvx html2text --ignore-links > "$tmpfile" 2>/dev/null
wc -c "$tmpfile"
head -c 15000 "$tmpfile"
```

If the page is larger than 15 000 characters, show both head and tail so the user can decide whether to read the full file:
```bash
echo "--- HEAD ---"
head -c 7500 "$tmpfile"
echo ""
echo "--- TAIL ---"
tail -c 7500 "$tmpfile"
echo ""
echo "(Full content saved to $tmpfile)"
```

## Rules

- Always quote search queries to avoid shell word-splitting.
- Respect robots.txt for scraping; do not hammer a host with repeated requests.
- Never send authentication cookies or session tokens to external URLs.
- If a page returns a login wall or CAPTCHA, report the URL and stop; do not attempt to bypass it.
