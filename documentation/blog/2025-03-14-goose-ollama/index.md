---
title: "AI, But Make It Local With goose and Ollama"
description: Integrate goose with Ollama for a fully local experience.
authors: 
    - tania
---

![blog cover](gooseollama.png)

On the [Goosing Around](https://youtube.com/playlist?list=PLyMFt_U2IX4uFFhd_2TD9-tlJkgHMMb6F&feature=shared) stream series, host [Rizel Scarlett](https://www.linkedin.com/in/rizel-bobb-semple/) [demonstrated how to use goose locally with Ollama](https://youtube.com/watch?v=WG10r2N0IwM?feature=share) for a fully local experience on your device. Her guest, [Parth Sareen](https://www.linkedin.com/in/parthsareen/), an experienced software engineer with a focus on building frameworks and systems for AI/ML, showed us the magic of structured outputs and how goose and Ollama work together under the hood.

<!--truncate-->

goose serves as an on-machine AI agent that can interact with your applications and tools through extensions, providing the framework and interface for AI-powered workflows. Ollama enables running large language models locally with a simple API, handling the optimization of models to run efficiently on consumer hardware.

Together, they create a self-contained AI agent workflow that puts advanced capabilities directly in the hands of developers.

# Getting Started

Before diving deep into various capabilities, Rizel walked us through how to set yourself up for success by integrating goose with Ollama. To follow along, you can [download goose](https://goose-docs.ai/) and follow a step-by-step walk through in the [Configure LLM Provider](https://goose-docs.ai/docs/getting-started/providers) guide.

If you have any questions or get stuck, feel free to chat with us on [Discord](https://discord.gg/goose-oss) or post an issue/discussion on [GitHub](https://github.com/aaif-goose/goose/). Thanks for reading!

# Why Go Local?
Using cloud-based LLMs and providers make it so you don't need substantial computing resources, so why go local? Here's some benefits you may want to consider:

- **True data privacy** since your conversations never leave your device. You have complete control over sensitive information. As Parth emphasized during the discussion, "Your data stays with you, period."
- **Offline capability** transforms when and where you can use AI. "I use Ollama all the time on planes—it's a lot of fun!" Parth shared, highlighting how local models free you from the constraints of internet connectivity.
- **Direct control over model behavior** means you can fine-tune parameters without subscription fees or API limits. Open source models allow you to get a closer look at what's happening behind the scenes.

Personal use cases like development assistance, personal knowledge management, education, and content management are but some examples that can benefit from working locally and offline. You can keep research and sensitive data private, and utilize goose when you have limited connectivity.

# Can My Machine Handle This?
This question came up repeatedly, and the answer is more encouraging than you think. As Parth pointed out, "You don't need to run the largest models to get excellent results." The requirements you'll want to look out for on your device boils down to this:

- **RAM is key**: 32GB is a solid baseline for larger models and outputs.
- **For MacBooks, RAM is your primary concern** given the unified memory architecture.
- **For Windows/Linux, GPU memory is more important** for acceleration

Use cases can start with smaller, more efficient models that run on modest hardware. Models optimized for efficiency can deliver impressive performance even on standard laptops! Just start with a smaller model to test your workflow, then scale up as you need. This way you can figure out if you need the beefy hardware or not.

# The Magic of Structured Outputs
Ollama supports [structured outputs](https://ollama.com/blog/structured-outputs), making it possible to constrain a model’s output to a specific format—essentially teaching models to respond in specific formats like JSON. Parth explained the concept with an elegant analogy: "It's like teaching someone math operations. You show them how to add, subtract, multiply, and then they can solve different problems following those patterns."

Parth showed us how these structured outputs can dramatically improve reliability. By constraining the model to respond within specific parameters, you get more consistent, predictable results. This structured approach ensures the model's response can be reliably parsed and integrated into applications—all while running locally on your device.

Here's an example of how to structure an output from the livestream:

```json
// Example of image analysis with structured output
{
  "scene": "sunset over mountains",
  "objects": [
    {
      "type": "sun",
      "attributes": ["orange", "setting", "partially visible"],
      "confidence": 0.95
    },
    {
      "type": "mountains",
      "attributes": ["silhouetted", "range", "distant"],
      "confidence": 0.92
    },
    {
      "type": "sky",
      "attributes": ["gradient", "orange to purple", "clear"],
      "confidence": 0.98
    }
  ],
  "mood": "peaceful",
  "lighting": "golden hour",
  "composition": "rule of thirds"
}
```
As Parth walked through these examples, he shared key practices to ensure you get the most out of local LLMs:

1. **For precision tasks, lower the temperature**. Setting it to 0 makes responses more deterministic and factual.
2. **Use structured outputs whenever possible**, be explicit about the format you want in your prompts.
3. **Be mindful of context windows**, local models have limits on how much information they can process at once.
4. **Experiment with different models**! Each has strengths and weaknesses you'll want to explore for your needs.
5. **For larger documents, chunk them** into manageable pieces, this helps a lot when you're working with larger files.

# It's About The Freedom To Choose
While there are trade-offs in terms of raw processing power when you go local vs cloud, you don't have to choose one over the other. As Parth summarized during the livestream: "Local AI isn't about replacing cloud options—it's about having the freedom to choose the right approach for your specific needs."

The benefits of owning your AI experience can be compelling for a variety of use cases. Whether you're a developer building tools, a writer working with confidential material, or simply someone who values privacy and control, I hope the goose-Ollama integration offers a glimpse into how a local experience can benefit you, and explore a future where sophisticated AI is as personal and private as the data on your hard drive. Thanks for reading!

<head>
  <meta property="og:title" content="Goosing Around: AI, But Make It Local With goose and Ollama" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/03/13/goose-ollama-local" />
  <meta property="og:description" content="Integrate goose with Ollama for a fully local experience." />
  <meta property="og:image" content="http://goose-docs.ai/assets/images/gooseollama-fbb2cb67117c81eaa189a6b6174e6c6c.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Goosing Around: AI, But Make It Local With goose and Ollama" />
  <meta name="twitter:description" content="Integrate goose with Ollama for a fully local experience." />
  <meta name="twitter:image" content="http://goose-docs.ai/assets/images/gooseollama-fbb2cb67117c81eaa189a6b6174e6c6c.png" />
</head>