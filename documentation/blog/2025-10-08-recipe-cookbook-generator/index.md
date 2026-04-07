---
title: "Build Your Own Recipe Cookbook Generator for goose"
description: Discover how goose can study your past sessions and turn your favorite workflows into reusable automation templates.
authors: 
    - ian
---

![Recipe Cookbook Generator](recipe-cookbook-generator.png)

You've been using goose for weeks, maybe months. You have dozens of successful [sessions](/docs/guides/sessions/) where you asked for help with blog posts, code reviews, documentation, or data analysis. Each time you think "Didn't I already do this?" but never get around to checking. Sound familiar?

I myself had over a hundred goose sessions and as many megabytes of conversation data. I was sitting on a goldmine of potential automation. A coworker suggested something brilliant: "What if goose could analyze your sessions and build recipes automatically?" Wait, wait, wait!! Create a personalized cookbook based on my own session history? Yes, please! Let's build a "cookbook generator" recipe!

<!--truncate-->

## The Problem with Manual Recipe Creation

Creating recipes manually is time-consuming, especially if you're looking through dozens or hundreds of previous sessions like I was, to do the following:

- Figure out which sessions were successful or not
- Extract the core workflow from lengthy conversations  
- Identify similar but not identical workflows and figure out what should be parameterized
- Write proper YAML syntax with templating, maybe build subrecipes?
- Test and refine the recipe structure

Isn't this the goal of having an AI agent in the first place? To save time and effort?

Let's boost our productivity by implementing some automation. We're gonna get goose to write a recipe that creates OTHER recipes!

## What is the Cookbook Generator Recipe?

The "cookbook generator" I'm describing here is a way to get goose to look at your previous sessions, analyze them for commonalities, and automatically create new recipes from common patterns. It's automation that creates automation -- meta-programming at its finest.

I'm going to share my own cookbook generator at the end.

Here's what it does:

1. **Scans your session history** -- Finds and reads all your goose session files
2. **Identifies successful workflows** -- Filters out incomplete or failed sessions
3. **Detects patterns** -- Groups similar requests and workflows together
4. **Generates parameterized recipes** -- Creates reusable YAML files with proper templating
5. **Handles sensitive data** -- Asks how to deal with file paths, API keys, and personal information
6. **Tracks progress** -- Remembers when it last ran to only process new sessions

The end result is a personalized cookbook of recipes tailored to your own specific workflows.

## The Vibe Prompting Process

[I demonstrated on a livestream](https://www.youtube.com/watch?v=-_1GALH2ER0) my whole process of creating this generator using "[vibe prompting](https://www.youtube.com/watch?v=IjXmT0W4f2Q)" -- having an extended conversation with goose to refine the idea and answer potential questions upfront. This approach uses fewer tokens than iterative coding, resulting in higher success rates.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/-_1GALH2ER0" title="Vibe Code with goose: Automate my workflow" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

During my own conversation with goose, goose asked smart questions like:

- **Pattern recognition**: How should we identify common workflows vs one-off tasks?
- **Granularity**: Should we create specific recipes or more general patterns?
- **Sensitive information**: How should we handle file paths, API keys, and personal data?
- **Reusability**: What parameters should be auto-detected vs user-specified?
- **Which sessions to consider**: I had an extended project that I didn't want included -- it was dozens of sessions about a single project, no point making a recipe for that.

By answering these questions upfront, we created a comprehensive specification before writing any code.

## Key Features of the Generator

### 1. Smart Session Analysis

The generator reads through your session files (typically stored in `~/.local/share/goose/sessions/` but this may vary on your platform, and may change in future goose releases) and analyzes:
- Overall outcome success
- User request patterns
- Tool usage sequences
- Common parameter types it could extract

This last one was really important for me. I ask goose all the time to help me build an outline for blog posts like this, or video scripts for our [YouTube channel](https://youtube.com/@goose-oss), or tutorial pages. They all follow a similar pattern, around a topic or subject, but the output format may vary.

### 2. Intelligent Filtering

Not every session should become a recipe. The generator should skip sessions that seem incomplete or abandoned, or compared with other sessions to determine if this was a one-off task. Still, what might appear to be a one-off task might actually be the start of something I want to repeat.

Having goose prompt me for whether I know ahead of time if there are any sessions I want to exclude was helpful -- I have several REALLY large, long sessions about the [community cookbook security scanner](/blog/2025/08/25/goose-became-its-own-watchdog/) I built, but I didn't want to build a recipe out of all of that.

Instead, I wanted goose to focus on workflows that appear multiple times, and ask me to confirm any cases that it was unsure about.

### 3. Parameterization Logic

The tool should automatically identify good candidates for parameters. I had ideas around the following things:

- Was I accessing file paths and directory structures regularly?
- Were there specific document types I was accessing, or trying to create? (blog posts, videos, documentation)
- Was I regularly accessing the same kinds of URLs and external resources
- Were there any common project names or topics, like MCP?

### 4. Template Generation

:::tip
I wanted goose to write all the recipes for me, but to be as up-to-date as possible. I cloned the [goose repository](https://github.com/aaif-goose/goose) and told goose to examine its own source code to learn how to successfully create recipes, and be sure to use proper YAML syntax.
:::

From that, I had goose think about the following ideas as it considered how to make my recipes:

- Parameter validation and defaults
- Conditional logic for different scenarios
- Loop constructs for repetitive tasks
- Subrecipe integration where appropriate

## Real-World Results

Building the cookbook generator took a little over an hour of "vibe prompting" and refining the idea, and then goose gave me a recipe. I always verify my AI-generated work, and then spent about 15 minutes more after the livestream refining some ideas and adding more guardrails to the recipe.

The recipes it generated for me were exactly as I had predicted:

- **Outline Generator**: parameterized for what kind of content I was making: a blog post, a video (and what kind), or tutorial documentation
- **Open Source Code Generator**: I often make little code samples to work alongside my blog posts or need code to demonstrate a concept in a video, but might change up the programming language. Nah, who am I kidding, it's always gonna be Python for me.
- **Research Assistant**: I frequently ask for help gathering information on a topic, but the scope and depth varies. This recipe lets me specify how deep to go and what kinds of sources to prioritize.
- **Image Generation**: I need thumbnail ideas for videos and blog posts, but the style and focus can vary widely. This recipe lets me specify the theme and mood.
- **Social Media Posts**: I struggle a lot with coming up with catchy social media posts, so I want to give goose the kind of content (blog, video, etc), point it at the content (in the case of video, I give it my narration script), and have it generate many different options that I can choose from later.

The recipes were about 90% workable, and I went through and refined some of those a bit further.

### Do the recipes ACTUALLY work though?

By running the outline generator recipe (three times), the image generator recipe, and social media recipe, goose took care of the following work:

- the outline for this blog post, so I could write the content faster
- the "conveyor belt" image at the top of this post (via Gemini)
- the social media posts you saw that brought you here
- the video script for the short video in the social media posts
- the script outline for a longer video going on our [YouTube channel](https://youtube.com/@goose-oss)

So, yeah. They work!

## The Meta-Automation Advantage

This approach represents a new level of AI-assisted productivity. Instead of manually identifying automation opportunities, the AI identifies them for you. It's like having a productivity consultant that never sleeps, that can analyze your work patterns on demand, and suggest ways of automating things.

:::info
If you want to try some hands-free automation, check out the experimental [Perception](https://github.com/michaelneale/goose-perception) project from one of our teammates!
:::

The cookbook generator also handles the tedious parts of recipe creation:

- Proper YAML syntax and structure
- Parameter type definitions and validation
- Template logic and conditional statements
- Extension requirements and dependencies

## Incremental Updates

The cookbook generator tracks when it last ran according to the output folder where you tell it to build recipes, so subsequent executions only analyze new sessions. This makes regular updates efficient and practical.

### Future Enhancements? Roadmap?

Could we expand the idea from here? Absolutely!

- **Smart categorization** -- Automatically organize recipes by domain (content, code, data)
- **Quality scoring** -- Rank recipes by potential time savings and reuse frequency
- **Dependency detection** -- Identify better places for recipes to use sub-recipes -- maybe I want to do a blog post AND a video AND social media posts all from the same recipe
- **Performance optimization** -- Incremental analysis and caching for large session histories

The last thing I'd want to improve would be to have goose re-analyze the recipes it made with newer sessions to refine existing recipes, instead of creating new recipes each time it runs.

## The Meta-Programming Future

The recipe cookbook generator is just the beginning. As AI agents become more sophisticated, we'll see more tools that create tools, automation that builds automation, and meta-programming approaches that amplify human productivity.

The key insight is that AI agents shouldn't just execute tasks -- they should learn from those executions and help us build better systems. This generator turns your goose usage history into a productivity asset, creating a personalized automation toolkit from your actual work patterns.

Start building your own cookbook generator, and stop doing the same work twice. Your future self will thank you for the automation you create today.


## Contribute to Our Community Cookbook

Want to contribute your own recipes or improvements to the cookbook generator? Join our [Discord community](https://discord.gg/goose-oss) or check out our [GitHub repository](https://github.com/aaif-goose/goose) for more automation ideas. Join our [Hacktoberfest](https://github.com/aaif-goose/goose/issues/4705) event going on to contribute recipes and prompt ideas to get on our leaderboard to win some great prizes!


## My Own Cookbook Generator Recipe

Here's the cookbook generator that goose helped me create, plus my own notes. You could try using it as-is, but I think a better approach would be to try vibe prompting with goose yourself to go analyze your own session history to see what kind of automation you want to set up for yourself.

```yaml
version: "1.0.0"
title: Recipe Cookbook Generator
description: |
  Analyze your goose session history to automatically generate recipes from your common workflows. This tool examines your past interactions with goose, identifies repetitive patterns, and creates reusable recipes that can automate similar tasks in the future. Perfect for capturing your personal automation patterns and building a custom recipe library.

prompt: |
  I want to analyze my goose session history and create recipes from common workflows I've used. I've done a variety of work, and I'd like your help finding repetitive tasks that we can turn into goose recipes to build my own personal 'cookbook' based on my goose usage patterns.

  {% if work_priorities %}My work priorities are: {{ work_priorities }}{% endif %}
  {% if exclusion_keywords %}Please skip sessions containing these topics: {{ exclusion_keywords }}{% endif %}
  
  I want to organize the recipes {{ organization_preference }} and review them in {{ review_mode }} mode.
  
  The process should:
  1. Find and analyze my goose session files{% if session_storage_path %} (located at {{ session_storage_path }}){% endif %}
  2. Identify successful workflows, prioritizing those I use repeatedly (at least {{ min_workflow_frequency }} times)
  3. Distinguish between repetitive patterns (high priority) and one-off tasks (user choice)
  4. Let me choose which patterns to turn into recipes, and subtasks that could be turned into subrecipes
  5. Generate parameterized YAML recipes with proper templating
  6. Handle sensitive data appropriately
  {% if include_test_mode == "true" %}7. Test generated recipes against recent sessions to validate they work{% endif %}
  
  Store the generated recipes in {{ recipe_output_dir }} and track when this cookbook generator last ran so future runs only analyze new sessions.

parameters:
  - key: recipe_output_dir
    description: Directory where generated recipes should be saved
    input_type: string
    requirement: required
  - key: session_storage_path
    description: Path to goose session storage (will auto-detect if not provided)
    input_type: string
    requirement: optional
  - key: work_priorities
    description: What types of work should I prioritize? (e.g., content, research, analysis, automation, development)
    input_type: string
    requirement: optional
  - key: exclusion_keywords
    description: Skip sessions containing these words/topics (comma-separated)
    input_type: string
    requirement: optional
  - key: organization_preference
    description: How to organize recipes - flat, by_category, by_date, by_frequency
    input_type: string
    requirement: optional
    default: "by_category"
  - key: review_mode
    description: How to review findings - interactive, batch_file
    input_type: string
    requirement: optional
    default: "interactive"
  - key: min_workflow_frequency
    description: Minimum number of times a workflow pattern must appear to be considered
    input_type: string
    requirement: optional
    default: "2"
  - key: include_test_mode
    description: Whether to validate generated recipes against recent sessions
    input_type: string
    requirement: optional
    default: "true"

instructions: |
  You are helping the user build a 'cookbook' of goose recipes based on their actual usage patterns.
  
  ## Step 1: Setup and Discovery
  - Determine the goose session storage path (try default locations like ~/.local/share/goose/sessions, or ask the user)
  - Check if this is an incremental run by looking for last-run timestamp in recipe_output_dir
  - Look for existing analysis metadata file (cookbook_analysis.json) to reuse previous work
  - Identify which session files to analyze (all files or only newer than last run)
    - ignore 0-byte session files and corrupted/incomplete files
  - If exclusion_keywords provided, skip sessions containing those terms
  
  ## Step 2: Intelligent Session Analysis  
  - Parse .jsonl session files to extract user requests, tool usage patterns, and outcomes
  - Use LLM analysis to automatically tag sessions with:
    - Intent categories (research, content-creation, analysis, automation, development)
    - Topic domains (extracted from session content - don't hardcode domains)
    - Action patterns (analyze, summarize, generate, outline, compare, etc.)
    - Tool usage sequences that indicate workflow patterns
  - Identify successful workflows by looking for:
    - Iterative refinement patterns (request → refine → refine → stop)
    - File creation/export outcomes
    - User satisfaction indicators
    - Task completion signals
  - Group similar workflows by analyzing:
    - Semantic similarity in user requests ("give me 5 ideas for...", "help me outline...")
    - Tool usage sequences (web_scrape → analyze → summarize)
    - Content types and domains worked with
    - Parameter patterns (file paths, URLs, content types)
  - Categorize findings:
    - HIGH PRIORITY: Patterns occurring >= min_workflow_frequency times
    - USER CHOICE: One-off tasks that might be worth generalizing
  - Focus on work_priorities if specified by user
  
  ## Step 3: User Review Process
  Based on review_mode parameter:
  
  ### Interactive Mode (default):
  - Present HIGH PRIORITY patterns first with numbering:
    - "1. Content Ideas Generator (Used 5 times)"
    - Brief description: "Generate ideas for blog posts, videos, workshops"
    - Parameters needed: content_type, topic_area, number_of_ideas
    - Sensitive data warnings if detected
  - Then present USER CHOICE patterns similarly
  - User can select by numbers: "1,3,5,7" or "all" or "none"
  
  ### Batch File Mode:
  - Generate a markdown summary file: "cookbook_review.md" in recipe_output_dir
  - Include all patterns with checkboxes: "- [ ] Content Ideas Generator..."
  - User edits file to check desired patterns
  - User tells you when ready to proceed
  
  ## Step 4: Recipe Generation with Subagents
  - For each confirmed pattern, launch a subagent to generate the recipe
  - Each subagent gets context about:
    - The specific workflow pattern
    - Example sessions that match the pattern
    - Suggested parameters and their default values
    - Organization preference for naming/structure
  - Subagents work in parallel to generate recipes faster
  - Each recipe includes:
    - Proper goose YAML format with version, title, description
    - Parameterized prompts using minijinja templating
    - Comprehensive instructions for headless mode
    - Custom metadata: cookbook_generator_metadata with source info
  - Generate subrecipes when workflows share common patterns
  - Ensure unique naming and handle subrecipe dependencies
  
  ## Step 5: Testing and Finalization
  - If include_test_mode is true:
    - Validate each generated recipe against recent matching sessions
    - Report any recipes that might not work as expected
  - Organize recipes according to organization_preference:
    - flat: All recipes in recipe_output_dir
    - by_category: Create subdirectories by workflow type
    - by_date: Organize by when patterns were first detected
    - by_frequency: Most frequent patterns in priority folders
  - Save analysis metadata to cookbook_analysis.json for incremental runs
  - Update last-run timestamp
  - Generate comprehensive README.md with:
    - Summary of generated recipes
    - Usage instructions
    - Parameter explanations
    - Tips for customization
  
  ## Advanced Guidelines:
  - SMART PATTERN DETECTION: Use LLM intelligence to find semantic patterns, not just keyword matching
  - PARAMETER INTELLIGENCE: Suggest default values based on most common values in sessions
  - SUBRECIPE OPPORTUNITIES: Identify shared workflow components for reusability
  - INCREMENTAL LEARNING: On subsequent runs, suggest updates to existing recipes
  - TOKEN EFFICIENCY: Summarize large sessions, focus on key patterns, batch similar analyses
  - CROSS-SESSION LEARNING: Weight patterns higher if they appear across different time periods
  - SENSITIVE DATA HANDLING: Detect API keys, file paths, personal info - warn but let user decide
  - RECIPE QUALITY: Ensure recipes are actionable, well-parameterized, and genuinely useful
  
  ## Pattern Recognition Intelligence:
  - Look for linguistic patterns: "give me X ideas", "help me outline", "research and summarize"
  - Detect workflow stages: ideation → planning → creation → refinement
  - Identify content domains from session content (don't assume specific domains)
  - Recognize tool usage signatures that indicate reusable processes
  - Find parameter opportunities in repeated values (paths, URLs, content types)
  - Distinguish setup tasks (one-time) from creative/analytical processes (repetitive)

extensions:
  - type: builtin
    name: developer
  - type: builtin
    name: dynamic_task

activities:
  - analyze_sessions
  - pattern_recognition  
  - recipe_generation
  - user_interaction
```


<head>
  <meta property="og:title" content="Build Your Own Recipe Cookbook Generator for goose" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/10/08/recipe-cookbook-generator" />
  <meta property="og:description" content="Discover how goose can study your past sessions and turn your favorite workflows into reusable automation templates." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/recipe-cookbook-generator-ba6ce396585e1fc591e339720ee80a30.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta name="twitter:title" content="Build Your Own Recipe Cookbook Generator for goose" />
  <meta name="twitter:description" content="Discover how goose can study your past sessions and turn your favorite workflows into reusable automation templates." />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/recipe-cookbook-generator-ba6ce396585e1fc591e339720ee80a30.png" />
</head>
