import { describe, expect, it, vi, beforeEach } from "vitest";
import React from "react";
import { render, screen } from "@testing-library/react";
import { App } from "../App";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { setupIpcMock, resetIpcMockState } from "../tests/mocks/ipc";
import { QueryClient } from "@tanstack/react-query";

beforeEach(() => { resetIpcMockState(); setupIpcMock(); })

describe("App", () => {
  it("renders heading and buttons", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } });
    render(<App client={qc} />);
    expect(screen.getByText(/Mgmt/)).toBeTruthy();
    expect(await screen.findByTestId('btn-tick')).toBeTruthy();
    expect(await screen.findByTestId('nav-ai')).toBeTruthy();
  });
});
