import React from "react";
import Admonition from "@theme/Admonition";

export const RateLimits = () => {
  return (
    <Admonition type="info" title="Billing">
      <a
        href="https://aistudio.google.com/app/apikey"
        target="_blank"
        rel="noopener noreferrer"
      >
        Google Gemini
      </a>{" "}
      offers a free tier you can get started with. Otherwise, you'll need to
      ensure that you have credits available in your LLM Provider account to
      successfully make requests.
      <br />
      <br />
      Some providers also have rate limits on API usage, which can affect your
      experience. Check out our{" "}
      <a href="/docs/guides/handling-llm-rate-limits-with-goose" target="_blank">
        Handling Rate Limits
      </a>{" "}
      guide to learn how to efficiently manage these limits while using goose.
    </Admonition>
  );
};
