import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  PromptInput,
  PromptInputSubmit,
  PromptInputTextarea,
} from "./prompt-input";

describe("PromptInputTextarea", () => {
  it("does not submit on Alt+Enter and inserts a newline", async () => {
    const onSubmit = vi.fn();
    const user = userEvent.setup();

    render(
      <PromptInput onSubmit={onSubmit}>
        <PromptInputTextarea />
        <PromptInputSubmit />
      </PromptInput>,
    );

    const input = screen.getByRole("textbox");
    await user.type(input, "hello");
    const wasNotPrevented = fireEvent.keyDown(input, {
      altKey: true,
      key: "Enter",
    });

    expect(wasNotPrevented).toBe(true);
    expect(onSubmit).not.toHaveBeenCalled();
    expect(input).toHaveValue("hello");
  });
});
