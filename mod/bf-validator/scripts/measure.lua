-- PT: Medição pelas estatísticas de produção da força: contagem total no início e no fim da
--     janela, convertida em itens por segundo.
-- EN: Measurement through the force production statistics: total count at the start and end of
--     the window, converted into items per second.

local measure = {}

-- PT: Contagens acumuladas de produção e consumo dos itens pedidos no teste.
-- EN: Accumulated production and consumption counts of the items requested by the test.
function measure.snapshot(test, surface_name)
  local surface = game.surfaces[surface_name]
  local items = game.forces.player.get_item_production_statistics(surface)
  local fluids = game.forces.player.get_fluid_production_statistics(surface)
  local counts = {}
  for _, name in ipairs(test.measure or {}) do
    local stats = prototypes.fluid[name] and fluids or items
    counts[name] = {
      produced = stats.get_input_count(name),
      consumed = stats.get_output_count(name),
    }
  end
  local network
  for _, pole in pairs(surface.find_entities_filtered({ type = "electric-pole" })) do
    network = pole.electric_network_statistics
    break
  end
  return {
    items = counts,
    electric_input = network and network.input_counts or {},
    electric_output = network and network.output_counts or {},
    pollution = game.get_pollution_statistics(surface).input_counts,
  }
end

-- PT: Taxas por segundo entre duas fotografias separadas por `ticks`.
-- EN: Per-second rates between two snapshots separated by `ticks`.
function measure.report(test, first, last, ticks)
  local seconds = ticks / 60
  local function per_second(a, b)
    local rates = {}
    for name, value in pairs(b) do
      rates[name] = (value - (a[name] or 0)) / seconds
    end
    return rates
  end

  local rates = {}
  for _, name in ipairs(test.measure or {}) do
    local a, b = first.items[name], last.items[name]
    rates[name] = {
      produced_per_second = (b.produced - a.produced) / seconds,
      consumed_per_second = (b.consumed - a.consumed) / seconds,
    }
  end
  return {
    seconds = seconds,
    rates = rates,
    electric_input_w = per_second(first.electric_input, last.electric_input),
    electric_output_w = per_second(first.electric_output, last.electric_output),
    pollution_per_second = per_second(first.pollution, last.pollution),
    machines = measure.machines(test),
  }
end

-- PT: Bônus de cada máquina de produção, em coordenadas da blueprint.
-- EN: Bonuses of each crafting machine, in blueprint coordinates.
function measure.machines(test)
  local surface = game.surfaces["bf-lab"]
  local offset = storage.built and storage.built.offset or { x = 0, y = 0 }
  local machines = {}
  for _, entity in pairs(surface.find_entities_filtered({ type = { "assembling-machine", "furnace", "rocket-silo" } })) do
    local recipe = entity.get_recipe()
    machines[#machines + 1] = {
      name = entity.name,
      x = entity.position.x - offset.x,
      y = entity.position.y - offset.y,
      recipe = recipe and recipe.name or nil,
      crafting_speed = entity.crafting_speed,
      speed_bonus = entity.speed_bonus,
      productivity_bonus = entity.productivity_bonus,
      consumption_bonus = entity.consumption_bonus,
      pollution_bonus = entity.pollution_bonus,
    }
  end
  return machines
end

return measure
