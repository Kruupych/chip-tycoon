import { vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'

type SaveRow = { id: number; name: string; created_at: string; progress: number; status?: string }

let monthIndex = 0
let saves: SaveRow[] = []
let nextSaveId = 1
let autosave = true

function nowIso() {
  return new Date().toISOString()
}

export function setupIpcMock() {
  (invoke as any).mockImplementation(async (cmd: string, payload?: any) => {
    switch (cmd) {
      case 'sim_lists':
        return { tech_nodes: ['N90', 'N65'], foundries: ['F1'], segments: ['Seg'] }
      case 'sim_state':
        return {
          date: '1990-01-01',
          month_index: monthIndex,
          companies: [{ name: 'A', cash_cents: 1000000, debt_cents: 0 }],
          segments: [{ name: 'Seg', base_demand_units: 1000, price_elasticity: -1.2, base_demand_t: 1000, ref_price_t_cents: 30000, elasticity: -1.2, trend_pct: 8.0, sold_units: 800 }],
          pricing: { asp_cents: 30000, unit_cost_cents: 20000 },
          kpi: { cash_cents: 1000000, revenue_cents: 0, cogs_cents: 0, contract_costs_cents: 0, profit_cents: 0, share: 0.2, rd_pct: 0.1, output_units: 1000, inventory_units: 950 },
          contracts: [],
          pipeline: { queue: [], released: [] },
          ai_plan: { decisions: ['ASP-5%'], expected_score: 0.5 },
          config: { finance: {}, product_cost: { usable_die_area_mm2: 6200, yield_overhead_frac: 0.05 } },
          campaign: null,
        }
      case 'sim_tutorial_state':
        return { active: false, current_step: 0, steps: [] }
      case 'sim_build_info':
        return { version: '0.1.0', git_sha: 'deadbeef', build_date: 'today' }
      case 'sim_help_markdown':
        return '# Help\n\nSome help text.'
      case 'sim_tick':
        monthIndex += (payload?.months ?? 1)
        return {
          months_run: monthIndex,
          cash_cents: 1000000,
          revenue_cents: 0,
          cogs_cents: 0,
          profit_cents: 0,
          contract_costs_cents: 0,
          asp_cents: 30000,
          unit_cost_cents: 20000,
          market_share: 0.2,
          rd_progress: 0.1,
          output_units: 1000,
          defect_units: 0,
          inventory_units: 950,
        }
      case 'sim_tick_quarter':
        monthIndex += 3
        return { months_run: monthIndex }
      case 'sim_plan_quarter':
        return { decisions: ['ASP-5%', 'Capacity+1000u/mo', 'Tapeout (expedite)'], expected_score: 0.42 }
      case 'sim_override':
        // Light payload validation to catch test regressions
        if (payload?.ovr?.price_delta_frac !== undefined && typeof payload.ovr.price_delta_frac !== 'number') throw new Error('price_delta_frac not number')
        if (payload?.ovr?.rd_delta_cents !== undefined && typeof payload.ovr.rd_delta_cents !== 'number') throw new Error('rd_delta_cents not number')
        return { asp_cents: 30000 }
      case 'sim_campaign_reset':
        monthIndex = 0
        return (await (invoke as any)('sim_state'))
      case 'sim_campaign_set_difficulty':
        return {}
      case 'sim_save':
        {
          const name: string = payload?.name || `manual-${nextSaveId}`
          const id = nextSaveId++
          const row: SaveRow = { id, name, created_at: nowIso(), progress: monthIndex, status: 'done' }
          saves = [row, ...saves]
          return id
        }
      case 'sim_list_saves':
        return saves
      case 'sim_load':
        return (await (invoke as any)('sim_state'))
      case 'sim_set_autosave':
        autosave = !!payload?.on
        return { enabled: autosave, max_kept: 6 }
      case 'sim_export_campaign':
        return {}
      case 'sim_balance_info':
        return { segments: [], active_mods: [] }
      default:
        return {}
    }
  })
}

export function getInvokeMock() {
  return (invoke as any) as ReturnType<typeof vi.fn>
}

export function resetIpcMockState() {
  monthIndex = 0
  saves = []
  nextSaveId = 1
  autosave = true
}
