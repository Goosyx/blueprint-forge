-- PT: Preparação do teste: superfície de laboratório, pesquisas, blueprint colada e construída,
--     energia infinita e pontos de entrada e saída (baús infinitos).
-- EN: Test preparation: lab surface, research, pasted and built blueprint, infinite power and
--     input and output points (infinity chests).

local setup = {}

-- PT: Cria (ou reaproveita) a superfície só com tiles de laboratório, sem inimigos nem recursos.
-- EN: Creates (or reuses) the surface with lab tiles only, no enemies and no resources.
local function lab_surface(name, radius_chunks)
  local surface = game.surfaces[name]
  if not surface then
    surface = game.create_surface(name, { width = 0, height = 0, no_enemies_mode = true })
  end
  surface.generate_with_lab_tiles = true
  surface.always_day = true
  surface.request_to_generate_chunks({ 0, 0 }, radius_chunks)
  surface.force_generate_chunk_requests()
  return surface
end

-- PT: Cola a blueprint e devolve os fantasmas criados.
-- EN: Pastes the blueprint and returns the created ghosts.
local function paste(surface, force, blueprint_string)
  local inventory = game.create_inventory(1)
  local stack = inventory[1]
  local imported = stack.import_stack(blueprint_string)
  if imported ~= 0 or not stack.is_blueprint then
    inventory.destroy()
    return nil, "import_failed"
  end
  local ghosts = stack.build_blueprint({
    surface = surface,
    force = force,
    position = { 0, 0 },
    build_mode = defines.build_mode.forced,
    skip_fog_of_war = false,
  })
  inventory.destroy()
  return ghosts
end

-- PT: Deslocamento entre as coordenadas da blueprint e as do mundo: o menor centro entre os
--     fantasmas menos o menor centro entre as entidades da blueprint (`anchor`).
-- EN: Offset between blueprint and world coordinates: the smallest center among the ghosts minus
--     the smallest center among the blueprint entities (`anchor`).
local function find_offset(ghosts, anchor)
  local min_x, min_y
  for _, ghost in pairs(ghosts) do
    if ghost.valid and ghost.name == "entity-ghost" then
      min_x = math.min(min_x or ghost.position.x, ghost.position.x)
      min_y = math.min(min_y or ghost.position.y, ghost.position.y)
    end
  end
  if not min_x then
    return nil
  end
  return { x = min_x - anchor.x, y = min_y - anchor.y }
end

-- PT: Modo `cover`: põe o poste do mod no centro das entidades da blueprint. Ele alcança 64 tiles
--     para cada lado; uma blueprint maior que isso fica com parte sem energia e o teste falha.
-- EN: `cover` mode: puts the mod's pole at the center of the blueprint's entities. It reaches 64
--     tiles each way; a larger blueprint keeps part unpowered and the test fails.
local function cover(surface, force)
  local min_x, min_y, max_x, max_y
  for _, entity in pairs(surface.find_entities_filtered({ force = force })) do
    local p = entity.position
    min_x = math.min(min_x or p.x, p.x)
    min_y = math.min(min_y or p.y, p.y)
    max_x = math.max(max_x or p.x, p.x)
    max_y = math.max(max_y or p.y, p.y)
  end
  if not min_x or max_x - min_x > 126 or max_y - min_y > 126 then
    return false
  end
  local pole = surface.create_entity({
    name = "bf-validator-pole",
    position = { (min_x + max_x) / 2, (min_y + max_y) / 2 },
    force = force,
  })
  return pole ~= nil
end

-- PT: Coloca uma interface de energia infinita dentro da área de alimentação de um poste.
-- EN: Places an infinite power interface inside a pole's supply area.
local function power(surface, force, mode)
  local poles = surface.find_entities_filtered({ type = "electric-pole", force = force })
  for _, pole in pairs(poles) do
    local reach = pole.prototype.get_supply_area_distance() - 1
    local position = surface.find_non_colliding_position("electric-energy-interface", pole.position, reach, 0.5)
    if position then
      local interface = surface.create_entity({
        name = "electric-energy-interface",
        position = position,
        force = force,
      })
      if interface then
        -- PT: `supply` alimenta a rede; `load` vira uma carga enorme para medir a geração da blueprint.
        -- EN: `supply` feeds the network; `load` becomes a huge load to measure the blueprint's generation.
        if mode == "load" then
          interface.power_production = 0
          interface.power_usage = 1e9
        else
          interface.power_production = 1e9
          interface.power_usage = 0
        end
        interface.electric_buffer_size = 1e12
        -- PT: Uma interface basta: os postes da blueprint estão ligados entre si.
        -- EN: One interface is enough: the blueprint's poles are wired together.
        return 1
      end
    end
  end
  return 0
end

-- PT: Os pedidos de itens da blueprint (módulos) viram "item-request-proxy" ao construir.
--     Aqui os itens são colocados direto no inventário de módulos e o pedido é removido.
-- EN: The blueprint's item requests (modules) become "item-request-proxy" entities when built.
--     Here the items are put straight into the module inventory and the request is removed.
local function insert_requested_items(surface)
  local inserted = 0
  for _, proxy in pairs(surface.find_entities_filtered({ name = "item-request-proxy" })) do
    local target = proxy.proxy_target
    local inventory = target and target.valid and target.get_module_inventory()
    if inventory then
      for _, request in pairs(proxy.item_requests) do
        inserted = inserted + inventory.insert({ name = request.name, count = request.count, quality = request.quality })
      end
    end
    proxy.destroy()
  end
  return inserted
end

-- PT: Pontos de teste: baús infinitos (`source` mantém itens; `sink` aceita e apaga tudo) e canos
--     infinitos (`fluid_source` mantém o fluido cheio; `fluid_sink` esvazia o fluido).
-- EN: Test points: infinity chests (`source` keeps items; `sink` accepts and deletes everything) and
--     infinity pipes (`fluid_source` keeps the fluid full; `fluid_sink` empties the fluid).
local function place_points(surface, force, points, offset)
  local placed = 0
  for _, point in ipairs(points or {}) do
    local position = { point.x + offset.x, point.y + offset.y }
    if point.kind == "fluid_source" or point.kind == "fluid_sink" then
      local pipe = surface.create_entity({ name = "infinity-pipe", position = position, force = force })
      if pipe then
        if point.kind == "fluid_source" then
          pipe.set_infinity_pipe_filter({
            name = point.fluid,
            percentage = 1,
            temperature = point.temperature,
            mode = "at-least",
          })
        else
          pipe.set_infinity_pipe_filter({ name = point.fluid, percentage = 0, mode = "exactly" })
        end
        placed = placed + 1
      end
    else
      local chest = surface.create_entity({ name = "infinity-chest", position = position, force = force })
      if chest then
        if point.kind == "sink" then
          chest.remove_unfiltered_items = true
        else
          for index, item in ipairs(point.items or {}) do
            chest.set_infinity_container_filter(index, {
              name = item,
              count = 200,
              mode = "at-least",
              index = index,
            })
          end
        end
        placed = placed + 1
      end
    end
  end
  return placed
end

-- PT: Executa a preparação inteira. Devolve uma tabela com `ok` e as contagens.
-- EN: Runs the whole preparation. Returns a table with `ok` and the counts.
function setup.run(test, surface_name)
  local surface = lab_surface(surface_name, test.radius_chunks or 3)
  local force = game.forces.player
  if test.research == "all" then
    force.research_all_technologies()
  end

  local ghosts, error_code = paste(surface, force, test.blueprint)
  if not ghosts then
    return { ok = false, error = error_code }
  end
  local offset = find_offset(ghosts, test.anchor)
  if not offset then
    return { ok = false, error = "anchor_not_found" }
  end

  local revived, failed = 0, 0
  for _, ghost in pairs(ghosts) do
    if ghost.valid and ghost.name == "entity-ghost" then
      local _, entity = ghost.revive({ raise_revive = false })
      if entity then
        revived = revived + 1
      else
        failed = failed + 1
      end
    end
  end

  local modules = insert_requested_items(surface)
  -- PT: Abastece as entidades a combustível para a rede não travar no começo (caldeira sem
  --     carvão não gera energia para o insersor que traria o carvão).
  -- EN: Fuels burner entities so the network does not deadlock at the start (a boiler without
  --     coal makes no power for the inserter that would bring the coal).
  local fueled = 0
  if test.prefuel then
    for _, entity in pairs(surface.find_entities_filtered({ force = force })) do
      local fuel = entity.get_fuel_inventory()
      if fuel then
        fueled = fueled + fuel.insert({ name = test.prefuel, count = 20 })
      end
    end
  end
  -- PT: O poste `cover` vem antes dos pontos de teste, para a caixa usar só a blueprint.
  -- EN: The `cover` pole comes before the test points, so the box uses only the blueprint.
  if test.power == "cover" and not cover(surface, force) then
    return { ok = false, error = "cover_failed" }
  end
  local points = place_points(surface, force, test.points, offset)
  local interfaces = power(surface, force, test.power)
  return {
    ok = true,
    revived = revived,
    failed = failed,
    points = points,
    modules = modules,
    fueled = fueled,
    power_interfaces = interfaces,
    offset = offset,
  }
end

return setup
