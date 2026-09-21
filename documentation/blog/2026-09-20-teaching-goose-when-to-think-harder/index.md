---
title: "Teaching goose when to think harder"
description: "We used a newly launched decision model to choose reasoning effort one turn at a time, and tested whether goose's state machine really makes uncertain agent experiments cheap to add and remove."
image: /img/blog/teaching-goose-when-to-think-harder.jpg
authors:
  - douwe
unlisted: true
---

![A goose selecting different-sized gears for one reasoning machine](/img/blog/teaching-goose-when-to-think-harder.jpg)

Automatic model routing is having a moment. Once an agent session makes dozens of model calls, using the strongest model for all of them gets expensive. A difficult refactor might deserve the best model available. Saying hello probably does not.

But switching models is a fairly blunt instrument. A different model can mean different tool-calling behavior, prompting quirks and context limits. It also means a different prompt cache. Reasoning models already expose another knob: keep the model fixed and vary how hard it thinks.

That is an attractive trade. Reasoning effort is already a range rather than a choice between unrelated models. The prompt, tools and model behavior stay the same. On APIs that support configuration updates, effort can now change without changing the cached prompt prefix. [OpenAI explicitly recommends this pattern](https://developers.openai.com/api/docs/guides/latest-model) when effort changes between responses.

So we had an experiment: could goose choose the reasoning effort for each user turn automatically?

<!-- truncate -->

This was also a test of the architecture. In [Agentic State Machines](/blog/2026/09/16/agentic-state-machines), I described replacing goose's central agent loop with ordered, cooperative Operations over persisted conversation state. The promise was not merely cleaner code. It was that we could try new agent behaviors without first teaching the whole agent about them.

Automatic effort was a good way to find out. It sounds like a small feature: classify the request, set an effort and call the model. Inside a traditional agent loop it quickly spreads. The classifier should run once per user turn, not once for every model call. Its answer must survive tool calls, retries and process restarts. Inference needs the selected setting. Cancellation and classifier failures need sensible behavior. If we are experimenting, we also need to see what it decided.

The architectural question was therefore more specific:

> Can we add automatic reasoning effort without putting automatic reasoning effort inside inference?

## A decision model for a decision

While we were thinking about this, TypeSafe [launched Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), its first System One model. Jev is not a chat model. It takes some state and one or more typed questions, then returns choices, scores or yes/no probabilities. A Choice response includes the selected option, the probability of every option and a confidence value. It does not generate an explanation that our code has to parse.

That is almost exactly the shape of our problem. We do not need a second model to solve the user's task. We need one quick judgment about the task:

```text
Choose the least thinking effort that can reliably handle this request.

off | low | medium | high | max
```

The options are ordered in our heads, but the result is still a typed choice the program can apply directly. Jev is designed for these small, focused judgments rather than long-form reasoning. It also returns the full distribution, so later we can decide what to do when the answer is uncertain instead of pretending every routing decision is equally trustworthy. TypeSafe's [primitives documentation](https://docs.typesafe.ai/primitives) explains the distinction.

The router also has to be much cheaper and faster than the work it is routing or it defeats the point. Jev is explicitly built for that role. Whether its choices are good enough is exactly what we want to find out.

## One Operation before inference

The implementation in [the experiment PR](https://github.com/aaif-goose/goose/pull/12237) is an `AutoEffortOperation` placed immediately before inference. In simplified form, it does this:

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

This matters during a tool loop. Suppose the user asks goose to make a code change. Automatic effort chooses `high`, then inference requests a tool. After the tool result is stored, the machine starts over and eventually reaches automatic effort again. The marker on the original user message says this turn has already been classified, so it passes. The second inference call uses the same `high` setting without making another Jev request.

The same thing happens after a restart because the decision is conversation metadata, not a boolean in a running coroutine. If Jev fails, goose keeps the configured effort. The feature is opt-in, requires its own API key and runs only for reasoning models. User requests are sent to a third-party service only when someone has explicitly enabled it.

## The test is the explanation

The lifecycle test tells the whole story better than a collection of unit tests could.

The first user message is "add one." The mocked Jev endpoint chooses `high`. Inference asks the calculator tool to add one, receives the result and runs again to produce the final answer. Both inference calls use high effort, but Jev is called exactly once.

The next user message is "hello." That begins a new turn, Jev chooses `off` and inference responds without reasoning harder than it needs to. The persisted conversation contains two full decisions and the client sees one short operation log on each resulting assistant message.

That scenario simultaneously checks turn boundaries, tool loops, persistence, model configuration and observability. More importantly, it checks that the behavior is state-machine behavior. If the implementation had hidden progress in a local variable, reconstructing the machine between steps would expose it.

## Cheap does not mean free

The automatic effort Operation is roughly 210 lines, but the pull request is much larger. That would make "we added this in one file" a good slogan and a bad description of the work.

This was the first experiment that needed an Operation to change session configuration. It was also the first one that needed a standard way to tell the client what an Operation had decided. The state machine did not support either yet.

We added two general capabilities:

- An Operation can return an effect that updates persisted model configuration.
- An Operation can attach a structured note to a message, including a short log that clients may display.

The full Jev decision remains in conversation metadata, where tests and future Operations can inspect it. The short `thinking high` log travels through ACP and appears in the message details UI. Neither mechanism knows what Jev is, and neither is limited to reasoning effort.

There was plenty of ordinary product work too: a setting and API-key control, ACP metadata transport, a desktop display and translations. An experimentation platform does not make product integration disappear.

The more useful property is that the experimental behavior still has one home. There is no Jev branch inside inference and no auto-effort flag threaded through the agent loop. If the experiment turns out to be a bad idea, we can remove the Operation and its setting. The generic effects it exposed remain useful machinery.

## What remains an experiment

The pull request proves that automatic effort can be expressed as a bounded, restart-safe behavior. It does not yet prove that Jev selects the right effort, that the extra latency pays for itself or that automatic effort is better than model routing.

Those are now questions we can answer with use rather than architecture. We can inspect which effort real turns receive, the returned confidence, the routing latency, cache usage, reasoning tokens and whether the task ultimately succeeds. A broader [decision-model interface for the Rust GDK](https://github.com/aaif-goose/goose/issues/12215) may eventually make sense, but we do not need to design that entire abstraction before trying one concrete behavior.

That is the part of the state-machine promise this experiment validates. It did not make the experiment free. It let the experiment teach us what the platform was missing without teaching the rest of the agent about the experiment.
