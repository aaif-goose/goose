import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "./dropdown-menu";

function SiblingMenus() {
  return (
    <div>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger>More</DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem>Duplicate</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger>Share</DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem>Copy file to clipboard</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

describe("DropdownMenu", () => {
  it("closes an open sibling menu when another menu opens", async () => {
    const user = userEvent.setup();
    render(<SiblingMenus />);

    await user.click(screen.getByRole("button", { name: "More" }));
    expect(
      screen.getByRole("menuitem", { name: "Duplicate" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Share" }));

    expect(
      screen.getByRole("menuitem", { name: "Copy file to clipboard" }),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(
        screen.queryByRole("menuitem", { name: "Duplicate" }),
      ).not.toBeInTheDocument();
    });
  });

  it("uses the same icon treatment for submenu triggers as menu items", async () => {
    const user = userEvent.setup();
    render(
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger>Actions</DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <svg aria-hidden="true" />
              Share
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem>Copy file to clipboard</DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        </DropdownMenuContent>
      </DropdownMenu>,
    );

    await user.click(screen.getByRole("button", { name: "Actions" }));

    const shareItem = screen.getByRole("menuitem", { name: "Share" });
    expect(shareItem).toHaveClass("gap-2");
    expect(shareItem.className).toContain("text-muted-foreground");
    expect(shareItem.className).toContain("[&_svg]:pointer-events-none");
    expect(shareItem.className).toContain(
      "[&_svg:not([class*='size-'])]:size-4",
    );
  });
});
