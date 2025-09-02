import { describe, expect, it, vi } from "vitest";
import React from "react";
import { render, screen } from "@testing-library/react";
import { App } from "../App";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("App", () => {
  it("renders heading and buttons", () => {
    render(<App />);
    expect(screen.getByText(/Chip Tycoon Mgmt UI/)).toBeTruthy();
    expect(screen.getByText(/Tick Month/)).toBeTruthy();
    expect(screen.getByText(/AI Plan/)).toBeTruthy();
  });
});

