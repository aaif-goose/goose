---
title: Building a Social Media Agent
description: I built a fully automated social media agent using MCP servers to fetch content and post through Sprout Social.
authors: 
    - ebony
---

![blog cover](header-image.png) 

> Creating content is fun.  
> Promoting it (aka the most important part) drains my soul 😩


When I posted that on LinkedIn the other night, I realized I'm definitely not the only one who feels this way. You spend hours making this masterpiece, and then you have to remember to promote it across multiple platforms every single time.

It’s exhausting, so I decided to automate it.

<!-- truncate -->

## The Game Plan

Here's what we're building: two [MCP servers](https://modelcontextprotocol.io/docs/getting-started/intro) that work together to handle all our social media promotion automatically.

**MCP Server #1: Content Fetcher**  
This one goes out and grabs all our content from:
- YouTube videos
- Blog posts  
- GitHub release notes

Then it compares everything to a `last_seen.json` file to figure out what's actually new. If nothing is new it proceeds to check an `evergreen.json` file and randomly pick old content to socialize.

**MCP Server #2: Sprout Social Integration**  
Once we have new content, this server takes over and:
- Generates captions for each platform
- Uploads media (videos, images, or just links)
- Creates draft posts in Sprout Social

The goal? Wake up to social posts ready to go, without lifting a finger. Well, almost, more on that later.

## Building the Content Fetcher

I used [Fast MCP](https://github.com/punkpeye/fastmcp) to spin up these TypeScript servers because, well, I'm a TypeScript girly. But you can use whatever SDK you vibe with.

First thing I needed was our YouTube channel ID. Quick tip: go to your YouTube channel, click on videos, and look at the URL. Everything after `/channel/` is your channel ID. Easy.

<details>
<summary>Click to see the code</summary>
```typescript

// fetch youtube function
async function fetchYoutube(): Promise<ContentItem[]> {
  const feed = await rssParser.parseURL(
    `https://www.youtube.com/feeds/videos.xml?channel_id=${YOUTUBE_CHANNEL_ID}`
  );

  return feed.items.map((item) => ({
    id: item.id || item.link || "",
    title: item.title || "",
    url: item.link || "",
    published_at: item.pubDate || "",
    type: "video" as const,
  }));
}

// Fetching YouTube videos tool
server.addTool({
  name: "fetchYoutube",
  description: "Fetch ALL YouTube videos from the goose channel.",
  parameters: z.object({}),
  execute: async () => JSON.stringify(await fetchYoutube()),
});
```
</details>

Same pattern for blogs and GitHub releases, straightforward tool functions with clear descriptions. The key is making your tool descriptions super simple and direct. goose needs to know exactly what each tool does.

The `last_seen.json` file is our source of truth. It tracks everything we've already promoted so we don't spam people with the same content over and over.

## The Sprout Social Side

This one needed way more setup. You need:
- API token (admin access required)
- Customer ID  
- Profile IDs for each social platform

Getting these IDs requires a curl command with your API token. I'll be honest - I should have read the docs first. Would've saved me some heartache.

<details>
<summary>Click to see the code</summary>
```typescript
server.addTool({
  name: "createScheduledPost",
  description:
    "Create a DRAFT post in Sprout scheduled for the future. Uses SCHEDULED delivery.",
  parameters: z.object({
    text: z
      .string()
      .describe("Text of the post. This will be the copy for the social post."),
    customer_profile_ids: z
      .array(z.number())
      .nonempty()
      .describe(
        "Array of Sprout customer_profile_ids to post to (e.g., LinkedIn, X, YouTube, Bluesky)."
      ),
    scheduled_times: z
      .array(z.string())
      .nonempty()
      .describe(
        "Array of ISO8601 UTC timestamps for scheduled send times (e.g. '2025-11-20T15:00:00Z')."
      ),
    media: z
      .array(
        z.object({
          media_id: z
            .string()
            .describe("media_id returned from uploadMediaFromUrl."),
          media_type: z
            .enum(["PHOTO", "VIDEO"])
            .describe("Type of media (PHOTO or VIDEO)."),
        })
      )
      .optional()
      .describe("Optional array of media to attach to the post."),
  }),
  execute: async ({ text, customer_profile_ids, scheduled_times, media }) => {
    try {
      const payload = buildPublishingPostPayload({
        text,
        customer_profile_ids,
        is_draft: true,
        scheduled_times,
        media,
      });

      const data = await sproutPost("/publishing/posts", payload);

      return JSON.stringify({
        success: true,
        request: payload,
        response: data,
      });
    } catch (err: any) {
      return JSON.stringify({
        success: false,
        error: err?.message || String(err),
      });
    }
  },
});
```
</details>

Here's where Sprout kind of did me dirty though. Their API doesn't let you create fully scheduled posts without human intervention. Everything has to go through as a draft first. I get it, brand safety and all that, but it's not the fully automated dream I was going for.

## Testing It Out

Once both MCP servers were built, I plugged them into goose. For local servers, you just:

1. Go to Extensions in goose
2. Add the server with `node` command and path to your server
3. Add any environment variables
4. Toggle it on

Then I asked goose: "Hey, can you tell me if we have any new content?"

And it just... worked. It hit all the tools, checked the `last_seen.json`, and came back with new releases, blog posts, and YouTube videos. Seeing those green checkmarks was *chef's kiss*.

> Here’s one of the drafts created in Sprout while I was testing
![Sprout draft screenshot](screenshot.png)

## So How Do We Actually Automate This?

Once both MCP servers were built, I still needed something to pull them together. MCP servers do not talk to each other on their own. Without goose and an orchestrating recipe, they are just two separate tools waiting to be called.

At first I created a setup with multiple subrecipes, each handling one part of the workflow. It technically worked, but it felt heavier than it needed to be.

After the livestream I stepped back and realized I could simplify everything. Instead of stitching together six different subrecipes, I built one single recipe that handles the entire flow in one place. It fetches content, decides what to post, generates captions, creates Sprout drafts, and updates the tracking file.

Sometimes the right move is to reduce instead of add, and this new version ended up being the cleanest and most reliable way to automate the whole process.

:::tip Don’t Forget to Schedule It

To fully automate this workflow, you must schedule your recipe. 
In goose Desktop, open the `recipe` section, click the `calendar icon` , and choose when it should run (I set mine to 10 AM daily).

You can read more in the [Reusable Recipes Guide](https://block.github.io/goose/docs/guides/recipes/session-recipes#schedule-recipe).
:::

<details>
<summary>Click to see the full daily automation recipe</summary>
```yaml
version: "1.0.0"
title: "Daily Social Promo Automation"
description: "Fetches new goose content or posts evergreen, generates platform-specific captions, and creates Sprout drafts."

instructions: |
  You are Ebony's daily social media automation assistant.
  
  ## YOUR WORKFLOW:
  
  ### STEP 1: Fetch All Content
  Call these MCP tools to gather everything:
  - contentfetcher__fetchYoutube
  - contentfetcher__fetchGooseBlog  
  - contentfetcher__fetchGithubReleases
  
  Each returns a JSON array. Combine them into one array of items with:
  { id, title, url, published_at, type }
  
  ### STEP 2: Check What's New
  For EACH item in your combined array:
  - Call contentfetcher__isNewContent with { id, type }
  - It returns { is_new: true/false }
  - Build a list of items where is_new == true
  
  ### STEP 3: Decide What to Post
  
  **IF you found NEW content:**
  - Pick the MOST RECENT new item (by published_at date)
  - Use that item for posting
  
  **IF NO new content exists:**
  - Load the file /Users/ebonyl/.config/goose/evergreen.json
  - Parse the JSON array
  - Randomly select ONE item from the array
  - Use that item for posting
  
  ### STEP 4: Generate Platform-Specific Captions
  
  For the selected item, create 3 captions following these rules:
  
  #### EBONY'S TONE (ALL PLATFORMS):
  - Confident, warm, developer-focused
  - NO hype language (never: "revolutionary", "unlock", "cutting-edge", "game-changer", "transform")
  - NO cringe marketing speak ("leverage", "synergy", "disrupt")
  - Short, clear sentences
  - 0-1 emoji maximum (✨ only, if any)
  - Never more than 1 exclamation point per post
  - Sound calm, resourceful, dev-first
  - Highlight what developers will LEARN or BUILD, not hype
  - Never use generic AI clichés ("fast-paced world", "stay ahead of the curve")
  - NEVER use em dashes (—) at all
  - Focus on practical value and real use cases
  - Be conversational but professional
  
  #### LINKEDIN RULES:
  - NEVER post YouTube links (heavily penalized by LinkedIn algorithm)
  - For videos: MUST use native video upload
  - Tone: calm, clear, slightly longer is OK (but still concise)
  - No more than 1 emoji
  - NO hashtags
  - Focus on professional learning value
  - Can be 2-3 sentences
  
  #### TWITTER/X RULES:
  - NEVER post YouTube links (penalized)
  - For videos: MUST use native video upload
  - Short and punchy (under 280 chars ideal)
  - No corporate tone
  - 0-1 emoji max
  - If thread needed: max 2 tweets
  - Conversational but professional
  - Get to the point fast
  
  #### BLUESKY RULES:
  - Links ARE allowed (YouTube links OK here)
  - Most conversational and casual
  - Emojis allowed if on-brand (still max 1)
  - For videos: prefer native upload but link is acceptable
  - Can be slightly more playful than other platforms
  - Community-focused tone
  
  #### MEDIA HANDLING BY CONTENT TYPE:
  
  **If type == "video" (YouTube content):**
  
  CRITICAL: YouTube URLs cannot be uploaded as native media to Sprout.
  You MUST handle each platform differently:
  
  - **LinkedIn:** 
    • DO NOT include YouTube URL in caption (penalized)
    • DO NOT pass media_url (cannot upload YouTube natively)
    • Caption should describe the video content
    • Say something like "Watch the full video on YouTube" WITHOUT the link
    • media_url: omit or empty string ""
  
  - **Twitter:**
    • DO NOT include YouTube URL in caption (penalized)
    • DO NOT pass media_url (cannot upload YouTube natively)
    • Caption should describe the video content
    • Say something like "Full video on YouTube" WITHOUT the link
    • media_url: omit or empty string ""
  
  - **Bluesky:**
    • Links ARE allowed here
    • Include the YouTube URL directly in the caption text
    • DO NOT pass media_url (cannot upload YouTube natively)
    • Caption should include the YouTube link
    • media_url: omit or empty string ""
  
  **If type == "blog":**
  - LinkedIn: include blog URL in caption text, no media_url
  - Twitter: include blog URL in caption text, no media_url
  - Bluesky: include blog URL in caption text, no media_url
  
  **If type == "release":**
  - LinkedIn: include release URL in caption text, no media_url
  - Twitter: include release URL in caption text, no media_url
  - Bluesky: include release URL in caption text, no media_url
  
  **IMPORTANT:** The sproutsocialmedia__createPostFromContent tool will:
  - Upload media natively IF you provide a direct media file URL (MP4, JPG, PNG, etc.)
  - YouTube URLs are NOT direct media files and cannot be uploaded
  - For YouTube videos, you must rely on caption text only (with link on Bluesky)
  
  ### STEP 5: Get Sprout Profile IDs
  
  Call sproutsocialmedia__getConfiguredProfiles to get the profile IDs.
  This returns:
  {
    linkedin_company: "<id>",
    twitter: "<id>",
    youtube: "<id>",
    bluesky: "<id>"
  }
  
  ### STEP 6: Create Sprout Drafts
  
  For EACH platform (linkedin, twitter, bluesky):
  
  Call sproutsocialmedia__createPostFromContent with:
  - caption: the platform-specific caption you generated (with URL in text if appropriate)
  - customer_profile_ids: [<the numeric profile ID for this platform>]
    • LinkedIn → use linkedin_company ID
    • Twitter → use twitter ID
    • Bluesky → use bluesky ID
  - media_url: ONLY if you have a direct media file URL (MP4, JPG, PNG, etc.)
    • For YouTube videos: DO NOT pass media_url (cannot upload YouTube URLs)
    • For blog posts: DO NOT pass media_url
    • For releases: DO NOT pass media_url
  - media_type: ONLY if you passed media_url
    • "VIDEO" for video files
    • "PHOTO" for image files
  - schedule_time: omit (creates draft, not scheduled)
  
  The MCP server will:
  - Upload media natively if media_url is a direct file URL
  - Create draft posts in Sprout
  - Return success confirmation
  
  REMEMBER: For YouTube videos, the link goes IN THE CAPTION TEXT (Bluesky only), 
  NOT as media_url!
  
  ### STEP 7: Mark as Seen
  
  **IF the item was NEW content (not evergreen):**
  - Call contentfetcher__markContentSeen with { id, type }
  - This updates ~/.config/goose/content-fetcher-mcp/last_seen.json
  
  **IF the item was EVERGREEN:**
  - DO NOT mark as seen (so it can be reused in the future)
  
  ### STEP 8: Summary
  
  Report what you posted:
  - Item title and type
  - Whether it was new or evergreen
  - Which platforms received posts (LinkedIn, Twitter, Bluesky)
  - Any errors encountered
  - Confirmation that item was marked as seen (if applicable)

prompt: |
  Begin today's scheduled social automation. Follow the workflow step by step.

extensions:
  - type: stdio
    name: contentfetcher
    cmd: node
    args:
      - /Users/ebonyl/content-fetcher-mcp2/dist/server.js
    timeout: 300
    description: "Fetches YouTube, blog, GitHub content and tracks what's been posted"

  - type: stdio
    name: sproutsocialmedia
    cmd: node
    args:
      - /Users/ebonyl/sprout-social-mcp/dist/server.js
    timeout: 300
    description: "Creates draft posts in Sprout Social"
    env_keys:
      - SPROUT_API_TOKEN
      - SPROUT_CUSTOMER_ID
      - SPROUT_GROUP_ID
      - SPROUT_PROFILE_ID_LINKEDIN
      - SPROUT_PROFILE_ID_TWITTER
      - SPROUT_PROFILE_ID_BLUESKY
      - SPROUT_PROFILE_ID_YOUTUBE

activities:
  - "Fetching latest goose content from all sources"
  - "Checking for new items against last_seen.json"
  - "Generating platform-specific captions with Ebony's tone"
  - "Creating draft posts in Sprout Social"
  - "Updating last_seen.json for posted items"

```
</details>

## Writing Like a Human

Here's something important, we don't want people to clock that it's automated. So I added specific rules:

- Zero or one emoji max (and really just ✨)
- Sound calm and resourceful, dev-first mentality
- No "in this fast-paced world" or "leverage technology" nonsense
- No hashtags unless actually justified
- Don't be too grammatically perfect so no em dashes (ironically)

Platform specific rules too:
- **LinkedIn**: No YouTube links (they penalize you), longer format okay
- **Twitter/X**: No YouTube links, Keep it concise, one emoji max
- **Blue Sky**: Links are fine here

## The Hiccups

Of course, nothing works perfectly on the first try. When I ran the recipe, I hit a few issues:

1. It wanted to post ALL nine new pieces of content at once and we don't want to spam people
2. For videos links were showing up instead of native media uploads

The Sprout draft requirement is still a bummer. Someone has to go in and toggle off the draft button before posts go live. Not ideal, but it still eliminates like 90% of the work.

## What's Next

I need to add:
- Logic to limit posts per day (maybe 2 max)
- Better handling of the evergreen content pool, once used we need to add some kind of tracking
- Fix the media upload flow for videos, I'm thinking of adding a Cloudflare R2 step

## The Vibe

This whole project took maybe an evening of focused coding, and now we have an agent that handles social promotion automatically. Is it perfect? No. But it's pretty damn close.

The best part? You can take this same approach for whatever automation you need. Spin up some MCP servers, create a recipe, let goose handle the orchestration. It's honestly so much fun watching it all come together.

If you want to try this yourself, I'll be sharing the GitHub repo with all the code. You'll need your own Sprout Social API key, but I'll put the setup steps in the readme.

And hey, if you figure out a way to get around that draft requirement, let me know. I'd love to make this truly hands off.

## Watch the Full Stream

Want to see the whole coding session? Check out the livestream where I built this live (with all the debugging and plant commentary):

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/49XLnhaxOMs" title="Vibe Code With Me | Build a Social Media Agent" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

Got questions or ideas? Come chat with us on [Discord](https://discord.gg/block-opensource) I'd love to hear what you're building!

<head>
  <meta property="og:title" content="Building a Social Media Agent" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://block.github.io/goose/blog/2025/11/21/building-social-media-agent" />
  <meta property="og:description" content="I built a fully automated social media agent using MCP servers to fetch content and post through Sprout Social." />
  <meta property="og:image" content="https://block.github.io/goose/assets/images/header-image-7f5ab50f65332fb53302ca30a3f86e46.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="block.github.io/goose" />
  <meta name="twitter:title" content="Building a Social Media Agent" />
  <meta name="twitter:description" content="I built a fully automated social media agent using MCP servers to fetch content and post through Sprout Social." />
  <meta name="twitter:image" content="https://block.github.io/goose/assets/images/header-image-7f5ab50f65332fb53302ca30a3f86e46.png" />
</head>