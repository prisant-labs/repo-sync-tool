// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { OptionCard } from "@/components/recipes/option-card";

afterEach(cleanup);

describe("OptionCard", () => {
  it("shows the label and the description at once, with no hover or open step", () => {
    render(<OptionCard label="Fast-forward" description="Pull when it is safe." active={false} onSelect={() => {}} />);
    const card = screen.getByRole("button", { name: /Fast-forward/ });
    expect(card.textContent).toContain("Pull when it is safe.");
  });

  it("calls onSelect when an unchosen, enabled card is clicked", () => {
    const onSelect = vi.fn();
    render(<OptionCard label="Check only" description="d" active={false} onSelect={onSelect} />);
    fireEvent.click(screen.getByRole("button", { name: /Check only/ }));
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("is not clickable once chosen, because choosing the chosen option does nothing", () => {
    const onSelect = vi.fn();
    render(<OptionCard label="Inherit global" description="d" active onSelect={onSelect} />);
    const card = screen.getByRole("button", { name: /Inherit global/ });
    expect((card as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(card);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("is not clickable while the caller reports it disabled", () => {
    const onSelect = vi.fn();
    render(<OptionCard label="Merge" description="d" active={false} disabled onSelect={onSelect} />);
    const card = screen.getByRole("button", { name: /Merge/ });
    expect((card as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(card);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("dims a card without disabling it", () => {
    render(<OptionCard label="Dimmed" description="d" active={false} dimmed onSelect={() => {}} />);
    const card = screen.getByRole("button", { name: /Dimmed/ });
    expect((card as HTMLButtonElement).disabled).toBe(false);
  });
});
