// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { AddReposDialog } from "@/components/add-repos-dialog";

/**
 * BL-NI-95 (`Dialog` primitive had no accessible-name mechanism, closed in the
 * N7 consistency sweep): `AddReposDialog` names itself via `aria-labelledby`
 * from its own visible "Add repositories" heading, mirroring how `Drawer`'s
 * two consumers were fixed in PR #77. A screen reader must announce this
 * dialog by its actual heading rather than the generic "dialog".
 */

afterEach(cleanup);

describe("AddReposDialog accessible name", () => {
  it("names the dialog from its visible heading while open", () => {
    render(<AddReposDialog open onClose={vi.fn()} onAdded={vi.fn()} />);

    expect(screen.getByRole("dialog", { name: "Add repositories" })).toBeDefined();
    // The name comes from the actual on-screen heading, not a duplicate string.
    expect(screen.getByRole("heading", { name: "Add repositories" })).toBeDefined();
  });

  it("is not exposed as a dialog while closed (inert + aria-hidden)", () => {
    render(<AddReposDialog open={false} onClose={vi.fn()} onAdded={vi.fn()} />);

    expect(screen.queryByRole("dialog", { name: "Add repositories" })).toBeNull();
  });
});
