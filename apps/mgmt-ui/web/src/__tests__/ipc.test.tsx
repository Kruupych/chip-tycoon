import { describe, it, expect, vi, beforeEach } from 'vitest'
import React from 'react'
import { render, screen, fireEvent, within } from '@testing-library/react'
import { App } from '../App'
import { QueryClient } from '@tanstack/react-query'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { getInvokeMock, setupIpcMock, resetIpcMockState } from '../tests/mocks/ipc'

describe('IPC wiring', () => {
  beforeEach(() => {
    resetIpcMockState();
    setupIpcMock();
    (getInvokeMock() as any).mockClear();
  })

  it('Markets: applies price delta via sim_override', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } })
    render(<App client={qc} />)
    fireEvent.click(await screen.findByTestId('nav-markets'))
    const input = await screen.findByDisplayValue('0')
    fireEvent.change(input, { target: { value: '5' } })
    fireEvent.click(screen.getByTestId('btn-price-apply'))
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ price_delta_frac: 0.05 }) })
    })
  })

  it('R&D: adjusts budget via sim_override', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } })
    render(<App client={qc} />)
    fireEvent.click(await screen.findByTestId('nav-rd'))
    const input = screen.getAllByRole('spinbutton')[0]
    fireEvent.change(input, { target: { value: '1234' } })
    fireEvent.click(screen.getByTestId('btn-rd-apply'))
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ rd_delta_cents: 1234 }) })
    })
  })

  it('Capacity: requests contract via sim_override', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } })
    render(<App client={qc} />)
    fireEvent.click(await screen.findByTestId('nav-capacity'))
    const inputs = screen.getAllByRole('spinbutton')
    fireEvent.change(inputs[0], { target: { value: '2000' } })
    fireEvent.change(inputs[1], { target: { value: '6' } })
    fireEvent.click(screen.getByTestId('btn-capacity-request'))
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ capacity_request: { wafers_per_month: 2000, months: 6 } }) })
    })
  })

  it('AI Plan: applies top decision via sim_override', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } })
    render(<App client={qc} />)
    fireEvent.click(await screen.findByTestId('nav-ai'))
    const btn = await screen.findByTestId('btn-ai-apply')
    fireEvent.click(btn)
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ price_delta_frac: expect.any(Number) }) })
    })
  })

  it('Campaign: reset and export JSON', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } })
    render(<App client={qc} />)
    fireEvent.click(await screen.findByTestId('nav-campaign'))
    fireEvent.click(await screen.findByTestId('btn-campaign-reset'))
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_campaign_reset', { which: '1990s' })
    })
    fireEvent.click(await screen.findByTestId('btn-export'))
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_export_campaign', expect.objectContaining({ path: expect.any(String) }))
    })
  })

  it('Tutorial: Load Tutorial triggers sim_campaign_reset("tutorial_24m")', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } })
    render(<App client={qc} />)
    // navigate to Tutorial page
    fireEvent.click(await screen.findByTestId('nav-tutorial'))
    const btn = await screen.findByText('Load Tutorial')
    fireEvent.click(btn)
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_campaign_reset', { which: 'tutorial_24m' })
    })
  })

  it('Save/Load: saves and loads, refetches state', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: 0 } } })
    render(<App client={qc} />)
    fireEvent.click(screen.getByTestId('btn-open-save'))
    const name = await screen.findByPlaceholderText('manual-...')
    fireEvent.change(name, { target: { value: 'manual-1' } })
    fireEvent.click(screen.getByTestId('btn-save'))
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_save', { name: 'manual-1' })
    })
    // wait for row to appear and click load
    const row = await screen.findByTestId('row-save')
    const loadBtn = within(row).getByTestId('btn-load')
    fireEvent.click(loadBtn)
    await vi.waitFor(() => {
      expect(getInvokeMock()).toHaveBeenCalledWith('sim_load', { save_id: expect.any(Number) })
    })
  })
})
