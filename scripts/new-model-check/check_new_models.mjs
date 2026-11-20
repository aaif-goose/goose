#!/usr/bin/env node
// Lists models that models.dev says were released in the last few days by the
// providers goose aims to support on launch day. Prints JSON for an agent to
// triage. It makes no judgment about whether goose handles the models.
//
// Usage: check_new_models.mjs <models.dev api.json file>
// Exit codes: 0 = nothing new, 10 = new models found, 1 = error.

import { readFile } from "node:fs/promises";

const DAYS = 3;
const PROVIDERS = ["anthropic", "openai", "google", "xai", "zai", "moonshotai", "deepseek"];

const DAY_MS = 24 * 60 * 60 * 1000;

// Full YYYY-MM-DD dates only, as UTC midnight; anything else is null.
function parseDate(text) {
  if (typeof text !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(text)) return null;
  const time = Date.parse(`${text}T00:00:00Z`);
  return new Date(time).toISOString().slice(0, 10) === text ? time : null;
}

function summarize(provider, id, model) {
  return {
    provider,
    model: id,
    name: model.name ?? null,
    description: model.description ?? null,
    family: model.family ?? null,
    release_date: model.release_date ?? null,
    tool_call: model.tool_call ?? null,
    temperature: model.temperature ?? null,
    input_modalities: model.modalities?.input ?? null,
    reasoning: model.reasoning ?? null,
    reasoning_options: model.reasoning_options ?? null,
    output_limit: model.limit?.output ?? null,
    context_limit: model.limit?.context ?? null,
  };
}

function findNewModels(catalog, now) {
  const today = now.toISOString().slice(0, 10);
  const since = new Date(parseDate(today) - DAYS * DAY_MS).toISOString().slice(0, 10);
  const newModels = [];
  const noReleaseDate = [];
  const missingProviders = [];

  for (const provider of PROVIDERS) {
    const models = catalog?.[provider]?.models;
    if (!models || typeof models !== "object") {
      missingProviders.push(provider);
      continue;
    }
    for (const [id, model] of Object.entries(models)) {
      if ((model.type ?? "chat") !== "chat") continue;
      const entry = summarize(provider, id, model);
      const released = parseDate(entry.release_date);
      if (released === null) noReleaseDate.push(entry);
      else if (released >= parseDate(since)) newModels.push(entry);
    }
  }
  if (missingProviders.length === PROVIDERS.length) {
    throw new Error("None of the providers were found in the models.dev data.");
  }

  newModels.sort((a, b) => b.release_date.localeCompare(a.release_date));
  return {
    checked_at: now.toISOString().replace(/\.\d{3}Z$/, "Z"),
    window: `models.dev models with release_date on or after ${since} (last ${DAYS} days, UTC)`,
    providers: PROVIDERS,
    new_models: newModels,
    no_release_date: noReleaseDate,
    missing_providers: missingProviders,
  };
}

async function main() {
  try {
    const file = process.argv[2];
    if (!file) throw new Error("usage: check_new_models.mjs <models.dev api.json file>");
    const result = findNewModels(JSON.parse(await readFile(file, "utf8")), new Date());
    console.log(JSON.stringify(result, null, 2));
    return result.new_models.length > 0 ? 10 : 0;
  } catch (error) {
    console.error(`Error: ${error.message}`);
    return 1;
  }
}

process.exitCode = await main();
