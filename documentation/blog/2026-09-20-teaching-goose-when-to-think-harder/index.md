---
title: "Teaching goose when to think harder"
description: "We used a newly launched decision model to choose reasoning effort one turn at a time, and tested whether goose's state machine really makes uncertain agent experiments cheap to add and remove."
image: /img/blog/teaching-goose-when-to-think-harder.jpg
authors:
  - douwe
unlisted: true
---

![A goose selecting different-sized gears for one reasoning machine](/img/blog/teaching-goose-when-to-think-harder.jpg)

[Automatic model routing](/blog/2026/04/20/mesh-llm) is having a moment. Six months ago companies were bragging about how many tokens they were using and running internal leaderboards for who was the most tokenmaxxed. Now CFOs everywhere are waking up to large LLM bills, and cost-cutting is becoming a thing in AI land. So using cheaper models for simpler tasks and reserving the most capable for things like tricky refactors has become an attractive idea.

But switching models is a blunt instrument. A different model can mean different tool-calling behavior, prompting quirks and context limits. Thinking blocks might not be preserved, especially when switching providers. It also means you blow away the cache, which can easily wipe out any cost savings. Reasoning models expose another knob: keep the model fixed and vary how hard it thinks.

That is an attractive trade. Reasoning effort is already a range rather than a choice between unrelated models. The prompt, tools and model behavior stay the same. On APIs that support configuration updates, effort can now change without changing the cached prompt prefix. [OpenAI explicitly recommends this pattern](https://developers.openai.com/api/docs/guides/latest-model) when effort changes between responses. Automating this is a nice experiment that should fit our recently released [Agentic State Machines](/blog/2026/09/16/agentic-state-machines) well.

<!-- truncate -->

As luck would have it, there's currently a lot of enthusiasm about decision models, and they are a good fit. TypeSafe [launched Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), its first System One model. Jev is not a chat model. It takes some state and one or more typed questions, then returns choices, scores or yes/no probabilities. A `Choice` response includes the selected option, the probability of every option and a confidence value.

That is almost exactly the shape of our problem. We do not need a second model to solve the user's task. We need one quick judgment about the task:

```text
Choose the least thinking effort that can reliably handle this request.

off | low | medium | high | max
```

The options are ordered in our heads, but the result is still a typed choice the program can apply directly. Decision models are designed for these small, focused judgments rather than long-form reasoning. They also return the full distribution, so later we can decide what to do when the answer is uncertain instead of pretending every routing decision is equally trustworthy. TypeSafe's [primitives documentation](https://docs.typesafe.ai/primitives) explains the distinction. These types of models are also fast and cheap.

## Operations as experiments

In goose's state machine architecture, effort selection fits naturally as an Operation. In [the experiment PR](https://github.com/aaif-goose/goose/pull/12237), this is an [`AutoEffortOperation`](https://github.com/aaif-goose/goose/blob/237b1ed163a19857b6b832f63e9efacc51b209ea/crates/goose/src/agents/state_machine/ops_auto_effort.rs) placed immediately before inference. In simplified form, it does this:

```text
if the kickoff message already has an effort decision:
    pass

decision = classify the kickoff message with Jev

return effects that:
    set the session's model configuration
    record the full decision on the kickoff message
    leave a short client-visible log
```

The state machine applies those effects and begins again from the top. When it reaches automatic effort a second time, the persisted decision is already on the kickoff message, so the Operation passes. Inference then reads the ordinary session model configuration and calls the existing provider with the selected effort.

Inference does not know that Jev exists. Tool execution does not know. Neither do retries, compaction or stop hooks.

This is the promise of Operations in practice. The experiment has one bounded home; the rest of goose only sees ordinary changes to persisted state.

This matters during a tool loop. Suppose the user asks goose to make a code change. Automatic effort chooses `high`, then inference requests a tool. After the tool result is stored, the machine starts over and eventually reaches automatic effort again. The marker on the original user message says this turn has already been classified, so it passes. The second inference call uses the same `high` setting without making another Jev request.

The same thing happens after a restart because the decision is conversation metadata, not a boolean in a running coroutine. If Jev fails, goose keeps the configured effort. The feature is opt-in, requires its own API key and runs only for reasoning models. User requests are sent to a third-party service only when someone has explicitly enabled it.

## The test is the explanation

The [lifecycle test](https://github.com/aaif-goose/goose/blob/237b1ed163a19857b6b832f63e9efacc51b209ea/crates/goose/src/agents/state_machine/tests/mod.rs#L161-L268) tells the whole story better than a collection of unit tests could.

The first user message is "add one." The mocked Jev endpoint chooses `high`. Inference asks the calculator tool to add one, receives the result and runs again to produce the final answer. Both inference calls use `high` effort, but Jev is called exactly once.

The next user message is "hello." That begins a new turn, Jev chooses `off` and inference responds without reasoning harder than it needs to. The persisted conversation contains two full decisions and the client sees one short operation log on each resulting assistant message.

That scenario simultaneously checks turn boundaries, tool loops, persistence, model configuration and observability. More importantly, it checks that the behavior is state-machine behavior. If the implementation had hidden progress in a local variable, reconstructing the machine between steps would expose it.

## New plumbing

The automatic effort Operation is roughly 210 lines, but integrating it with the desktop exposed some plumbing the state machine still needed. These changes should be useful for future experiments.

We added two general capabilities:

- An Operation can return an effect that updates persisted model configuration.
- An Operation can attach a structured note to a message, including a short log that clients may display.

Both should serve future experiments: Operations can update session configuration and tell the user what they changed without adding experiment-specific paths to the machine.

## The shape of an experiment

The pull request proves that automatic effort can be expressed as a bounded, restart-safe behavior. It does not yet prove that Jev selects the right effort, that the extra latency pays for itself or that automatic effort is better than model routing.

Those are now questions we can answer with use rather than architecture. We can inspect which effort real turns receive, the returned confidence, the routing latency, cache usage, reasoning tokens and whether the task ultimately succeeds. If automatic effort works, we have a useful new behavior. If it does not, we can remove one Operation and keep what we learned. Either way, we got to test the idea without first redesigning the agent around it.

# Wrapping Up

Automatic effort selection helps with cost, but also makes for a useful test of whether goose's state machine can absorb new ideas cheaply. One bounded Operation, two pieces of reusable plumbing, and a single lifecycle test were enough to let goose decide how hard to think without inference, tools, or retries ever knowing about it. Whether or not Jev picks the right effort, this is the kind of experiment we want goose to make easy.
