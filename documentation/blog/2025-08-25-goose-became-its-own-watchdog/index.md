---
title: "I had Goose Build its Own Secure Recipe Scanner"
description: Building community trust by having AI analyze AI recipes for safety
authors:
  - ian
---

![Goose Recipe Safety](goose-security-scanner.png)

Remember when people shared recipes by passing handwritten cards between neighbors? You trusted Grandma's apple pie recipe because you knew Grandma. But what happens when strangers start sharing recipes online? You need someone to taste-test them first.

That's exactly the challenge we face with Goose recipes. We're building a community cookbook where you can try Goose recipes from other users with confidence that they're safe. But we needed a way to make sure every recipe was safe to run.

<!--truncate-->

## The Headless Solution

I asked Goose: "Can you build a system to analyze your own recipes?"

The beautiful irony wasn't lost on me. I'm essentially asking our AI to become its own watchdog.

I gave it a lot more direction: I wanted the scanner to run from GitHub Actions, and I wanted the scanning to be done within a Docker container so it could inspect the recipe in an isolated environment.

After several high-level iterations on the concept and how it should work, Goose had built a complete security analysis system. It containerized itself, set up GitHub Actions workflows, and started scanning recipe submissions automatically.

### Better Prompting for Better Results

My first build was pretty over-engineered. I simplified later by just giving Goose a prompt of "you're a security expert," but the results of that didn't work as well without guiding it on what kinds of things it should be looking for. I had to step up my prompting game to include _some_ amount of specifics, but still giving Goose the flexibility to learn and grow and to download any tools it thought it needed to get the job done within the Docker container.

In the end I crafted a series of recipes that were safe, some that were maybe a little risky, and some that would be outright dangerous, and telling Goose some of the necessary things we want to watch out for.

## How It Works

The process at a glance seems surprisingly simple but it's pretty complex -- picture the graceful goose swimming on the water but under the water the feet are thrashing around doing lots of work!

When someone submits a recipe via our GitHub issue template, it will start an automated scan. Goose spins up in an isolated Docker container, analyzes the recipe using its own security expertise plus some of our guidance, scores it for risk, and posts the results right back to the GitHub issue.

The whole process takes minutes instead of days, and the submitter gets immediate, transparent feedback about their recipe's safety. If something looks off, our team can review what's going on, and take appropriate action.

## Goose in Headless Mode

We've covered headless mode in [tutorials](https://goose-docs.ai/docs/tutorials/headless-goose/) and [videos](https://www.youtube.com/@goose-oss/search?query=headless), but here's a quick recap: headless mode allows Goose to run without a graphical user interface, making it faster and more efficient for automated tasks. It excels in server environments as long as we're being _really_ clear about the instructions to follow, or a fallback if the instructions cannot be followed -- we don't want something half-finished or broken as a result if Goose gets stuck on what to do.

We launch the Docker container with something like this:

```bash
docker run --rm \
    -e AIMODEL_API_KEY="$AIMODEL_API_KEY" \
    -v "$PWD/$RECIPE_FILE:/input/recipe.yaml:ro" \
    -v "$RECIPE_OUT:/output" \
```

The first thing we're passing into Docker is the API key for whichever AI model we're using. i'm just using a placeholder of "AIMODEL_API_KEY" but you would change this to OPENAI_API_KEY or GEMINI_API_KEY etc depending on the LLM you want Goose to use in your container.

Next, we're passing in the user's recipe that we're getting from GitHub, and then we have our "output" for logs and analysis.

### Looking Inside the Container

Inside the container, we're installing Goose, and passing in a configuration file for which AI provider and model we want to use, plus a "base" recipe that tells Goose how to analyze the user's recipe. That recipe is also reinforcing a role of Goose being a security expert.

## The Learning Curve

Just telling Goose "you're a security expert" wasn't enough. It took some back-and-forth to teach the difference between a recipe that downloads a helpful development tool, and something that downloads something sketchy to your home directory to look for sensitive data.

We had to fine-tune the balance between security and usability. Too strict, and legitimate recipes get flagged. Too lenient, and dangerous ones slip through. Getting that balance right required showing Goose lots of examples of both safe and risky patterns. We pass those into the Docker container as well, and our "base" recipe tells Goose to use those as inspiration.

Then we go into "headless" mode:

```bash
goose run --recipe base_recipe.yaml --no-session --params recipe_path="user_recipe.yaml" > /logs/results.txt
```

This runs our "base" recipe, and skips storing a session since this is a one-off GitHub action anyway. Our base recipe looks for a parameter of where to find the user's recipe file, so we pass that parameter into our headless mode, and then we log the results. Those results are later picked up by our GitHub action to populate a comment on a GitHub issue or pull request.

## Building Community Trust

The real win isn't just the automation of all of this, it's the transparency. Every analysis is visible, consistent, and explained. Community members can see exactly why a recipe passed or failed, which builds trust in both the system and the recipes themselves.

Goose catches edge cases that humans might miss, like subtle obfuscation techniques or patterns that only become obvious when you're analyzing dozens of recipes. It's like having a security expert with perfect attention to detail who never gets tired.

## Using AI to review AI to review submissions

Sometimes the best way to solve a potential AI problem is with more AI. Goose understands Goose behavior patterns better than any human reviewer could. It knows the legitimate ways to automate tasks and can spot when something deviates from those patterns. Using Goose to build this scanner wasn't just a time-saver for making the tool, it's a productivity win for our team from manually review every recipe ourselves.

Anyone can submit a recipe knowing it'll get a fair, thorough review. And when you see a recipe get a security approval, remember: it was approved by Goose itself.

<iframe class="aspect-ratio" src="https://www.youtube.com/embed/Jtw_FxF3Iug" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>

<head>
  <meta property="og:title" content="I had Goose Build its Own Secure Recipe Scanner" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/kljaslkjasd" />
  <meta property="og:description" content="Goose headless mode runs a containerized scanner for community recipe submissions." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/goose-security-scanner-7fbe93f4a738fed2002e656fe66e715f.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="I had Goose Build its Own Secure Recipe Scanner" />
  <meta name="twitter:description" content="Goose headless mode runs a containerized scanner for community recipe submissions." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/goose-security-scanner-7fbe93f4a738fed2002e656fe66e715f.png" />
</head>
