import { describe, it, expect, vi, beforeEach } from 'vitest'
import React from 'react'
import { render, screen, fireEvent } from '@testing-library/react'
import { App } from '../App'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { invoke as invokeMock } from '@tauri-apps/api/core'

;(invokeMock as any).mockImplementation(async (cmd: string, payload?: any) => {
  switch (cmd) {
    case 'sim_state':
      return {
        date: '1990-01-01',
        month_index: 0,
        companies: [{ name: 'A', cash_cents: 1000000, debt_cents: 0 }],
        segments: [{ name: 'Seg', base_demand_units: 1000, price_elasticity: -1.2, base_demand_t: 1000, ref_price_t_cents: 30000, elasticity: -1.2, trend_pct: 8.0, sold_units: 800 }],
        pricing: { asp_cents: 30000, unit_cost_cents: 20000 },
        kpi: { cash_cents: 1000000, revenue_cents: 0, cogs_cents: 0, contract_costs_cents: 0, profit_cents: 0, share: 0.2, rd_pct: 0.1, output_units: 1000, inventory_units: 950 },
        contracts: [],
        pipeline: { queue: [], released: [] },
        ai_plan: { decisions: ['ASP-5%'], expected_score: 0.5 },
        config: { finance: {}, product_cost: { usable_die_area_mm2: 6200, yield_overhead_frac: 0.05 } },
        campaign: null,
      };
    case 'sim_lists':
      return { tech_nodes: ['N90'], foundries: [], segments: ['Seg'] };
    case 'sim_tutorial_state':
      return { active: false, current_step: 0, steps: [] };
    case 'sim_build_info':
      return { version: '0.1.0', git_sha: 'deadbeef', build_date: 'today' };
    case 'sim_plan_quarter':
      return { decisions: ['ASP-5%', 'Capacity+1000u/mo'], expected_score: 0.42 };
    case 'sim_override':
    case 'sim_tick':
    case 'sim_tick_quarter':
    case 'sim_save':
    case 'sim_list_saves':
    case 'sim_load':
    case 'sim_set_autosave':
      return {} as any;
    default:
      return {} as any;
  }
})

describe('IPC wiring', () => {
  beforeEach(() => {
    invokeMock.mockClear()
  })

  it('Markets: applies price delta via sim_override', async () => {
    render(<App />)
    fireEvent.click(await screen.findByText('Markets'))
    const input = await screen.findByDisplayValue('0')
    fireEvent.change(input, { target: { value: '5' } })
    fireEvent.click(screen.getByText('Apply'))
    expect(invokeMock).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ price_delta_frac: 0.05 }) })
  })

  it('R&D: adjusts budget via sim_override', async () => {
    render(<App />)
    fireEvent.click(await screen.findByText('R&D / Tapeout'))
    const input = screen.getAllByRole('spinbutton')[0]
    fireEvent.change(input, { target: { value: '1234' } })
    fireEvent.click(screen.getByText('Apply'))
    expect(invokeMock).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ rd_delta_cents: 1234 }) })
  })

  it('Capacity: requests contract via sim_override', async () => {
    render(<App />)
    fireEvent.click(await screen.findByText('Capacity'))
    const inputs = screen.getAllByRole('spinbutton')
    fireEvent.change(inputs[0], { target: { value: '2000' } })
    fireEvent.change(inputs[1], { target: { value: '6' } })
    fireEvent.click(screen.getByText('Request'))
    expect(invokeMock).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ capacity_request: { wafers_per_month: 2000, months: 6 } }) })
  })

  it('AI Plan: applies top decision via sim_override', async () => {
    render(<App />)
    fireEvent.click(await screen.findByText('AI Plan'))
    const btn = await screen.findByText('Apply Top Decision')
    fireEvent.click(btn)
    expect(invokeMock).toHaveBeenCalledWith('sim_override', { ovr: expect.objectContaining({ price_delta_frac: expect.any(Number) }) })
  })

  it.skip('Save/Load: saves game and lists saves', async () => {
    render(<App />)
    fireEvent.click(screen.getByText(/Save\/Load/))
    const name = await screen.findByPlaceholderText('manual-...')
    fireEvent.change(name, { target: { value: 'manual-1' } })
    fireEvent.click(screen.getByText('Save'))
    expect(invokeMock).toHaveBeenCalledWith('sim_save', { name: 'manual-1' })
    expect(invokeMock).toHaveBeenCalledWith('sim_list_saves')
  })
})
