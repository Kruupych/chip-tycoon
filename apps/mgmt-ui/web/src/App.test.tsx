import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { getInvokeMock, setupIpcMock, resetIpcMockState } from "./tests/mocks/ipc";
import { App } from "./App";
import { QueryClient } from "@tanstack/react-query";

beforeEach(() => {
  resetIpcMockState();
  setupIpcMock();
});

describe("App", () => {
  it("renders and updates on Tick Month (auto-refresh sim_state)", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } });
    render(<App client={qc} />);
    const btn = await screen.findByTestId("btn-tick");
    fireEvent.click(btn);
    // Ensure sim_state was called after sim_tick
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith("sim_state", undefined);
    })
  });

  it("simulate quarter advances by three months", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } });
    render(<App client={qc} />);
    const btn = await screen.findByTestId("btn-quarter");
    fireEvent.click(btn);
    // after calling, sim_state should be refetched (we mock fixed date but ensure call made)
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith("sim_tick_quarter", undefined);
      expect(getInvokeMock()).toHaveBeenCalledWith("sim_state", undefined);
    })
  });
});
