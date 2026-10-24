-- PT: Esteiras de entrada e saída dos testes. Uma esteira de entrada é a cabeça de esteira (sem
--     nada antes) a até 2,5 tiles de um combinador constante ligado: o 1º item das seções ativas
--     vai na faixa da esquerda e o 2º (ou o 1º de novo) na da direita. Uma esteira de saída é a
--     ponta sem nada à frente e sem nenhum insersor pegando no caminho até ela; o que chega nela
--     é apagado a cada tick, para a linha nunca entupir. Só use `auto_belts` em blueprints de
--     produção: ali todo combinador constante ligado marca uma entrada.
-- EN: Test input and output belts. An input belt is a belt head (nothing before it) within 2.5
--     tiles of an enabled constant combinator: the 1st item of the active sections goes on the
--     left lane and the 2nd (or the 1st again) on the right one. An output belt is the end with
--     nothing ahead and no inserter picking along the path to it; whatever reaches it is deleted
--     every tick, so the line never clogs. Only use `auto_belts` on production blueprints: there,
--     every enabled constant combinator marks an input.

local feed = {}

local VECTORS = { [0] = { 0, -1 }, [4] = { 1, 0 }, [8] = { 0, 1 }, [12] = { -1, 0 } }

-- PT: Itens de um combinador ligado, das seções ativas. Sinais virtuais e de fluido ficam de fora.
-- EN: Items of an enabled combinator, from its active sections. Virtual and fluid signals are left out.
local function combinator_items(combinator)
  local behavior = combinator.get_control_behavior()
  if not behavior or not behavior.enabled then
    return nil
  end
  local items = {}
  for _, section in pairs(behavior.sections) do
    if section.active then
      for _, filter in pairs(section.filters) do
        local value = filter.value
        if value and value.name and (value.type == nil or value.type == "item") and prototypes.item[value.name] then
          items[#items + 1] = value.name
        end
      end
    end
  end
  if #items == 0 then
    return nil
  end
  return { items[1], items[2] or items[1] }
end

-- PT: Acha as entradas e saídas na blueprint colada. Devolve a tabela guardada em `storage`, ou
--     nil e o motivo quando um combinador com item fica sem esteira.
-- EN: Finds the inputs and outputs in the pasted blueprint. Returns the table kept in `storage`,
--     or nil and the reason when a combinator with items has no belt.
function feed.detect(surface, force)
  local found = { sources = {}, sinks = {} }
  -- PT: Todos os pares (combinador, cabeça) a até 2,5 tiles, atribuídos do mais perto para o mais
  --     longe: a escolha não depende da ordem em que o jogo devolve as entidades.
  -- EN: Every (combinator, head) pair within 2.5 tiles, assigned from nearest to farthest: the
  --     choice does not depend on the order in which the game returns the entities.
  local pairs_found, wanted = {}, {}
  for _, combinator in pairs(surface.find_entities_filtered({ name = "constant-combinator", force = force })) do
    local items = combinator_items(combinator)
    if items then
      wanted[combinator.unit_number] = items
      for _, belt in pairs(surface.find_entities_filtered({
        type = "transport-belt",
        position = combinator.position,
        radius = 2.5,
        force = force,
      })) do
        if #belt.belt_neighbours.inputs == 0 then
          local dx = belt.position.x - combinator.position.x
          local dy = belt.position.y - combinator.position.y
          pairs_found[#pairs_found + 1] = { combinator = combinator.unit_number, belt = belt, distance = dx * dx + dy * dy }
        end
      end
    end
  end
  table.sort(pairs_found, function(a, b)
    if a.distance ~= b.distance then
      return a.distance < b.distance
    end
    return a.belt.unit_number < b.belt.unit_number
  end)
  local used_belts, served = {}, {}
  for _, pair in ipairs(pairs_found) do
    if not served[pair.combinator] and not used_belts[pair.belt.unit_number] then
      served[pair.combinator] = true
      used_belts[pair.belt.unit_number] = true
      found.sources[#found.sources + 1] = { belt = pair.belt, items = wanted[pair.combinator] }
    end
  end
  for number in pairs(wanted) do
    if not served[number] then
      return nil, "input_without_belt"
    end
  end

  -- PT: Tiles de onde algum insersor pega: a ponta de uma esteira de entrada também fica aberta,
  --     mas não pode ser esvaziada.
  -- EN: Tiles some inserter picks from: an input belt's end is also open, but must not be emptied.
  local picked = {}
  for _, inserter in pairs(surface.find_entities_filtered({ type = "inserter", force = force })) do
    local p = inserter.pickup_position
    picked[math.floor(p.x) .. "," .. math.floor(p.y)] = true
  end
  local function tile_of(entity)
    return math.floor(entity.position.x) .. "," .. math.floor(entity.position.y)
  end
  -- PT: Uma ponta só é saída se nenhum insersor pega em todo o caminho até ela; senão é uma
  --     esteira intermediária, que no jogo real enche e para.
  -- EN: An end is only an output if no inserter picks along the whole path to it; otherwise it is
  --     an intermediate belt, which in the real game fills up and stops.
  local function pure_output(last)
    local pending, seen = { last }, { [last.unit_number] = true }
    while #pending > 0 do
      local belt = table.remove(pending)
      if picked[tile_of(belt)] then
        return false
      end
      for _, input in pairs(belt.belt_neighbours.inputs) do
        if not seen[input.unit_number] then
          seen[input.unit_number] = true
          pending[#pending + 1] = input
        end
      end
    end
    return true
  end
  for _, belt in pairs(surface.find_entities_filtered({ type = "transport-belt", force = force })) do
    local vector = VECTORS[belt.direction]
    if vector and #belt.belt_neighbours.outputs == 0 and pure_output(belt) then
      local ahead = { belt.position.x + vector[1], belt.position.y + vector[2] }
      if #surface.find_entities_filtered({ position = ahead }) == 0 then
        found.sinks[#found.sinks + 1] = belt
      end
    end
  end
  return found
end

-- PT: Mantém as entradas cheias e esvazia as saídas. Roda a cada tick.
-- EN: Keeps the inputs full and empties the outputs. Runs every tick.
function feed.tick(found)
  for _, source in pairs(found.sources) do
    if source.belt.valid then
      for lane = 1, 2 do
        local line = source.belt.get_transport_line(lane)
        if line.can_insert_at_back() then
          line.insert_at_back({ name = source.items[lane], count = 1 })
        end
      end
    end
  end
  for _, sink in pairs(found.sinks) do
    if sink.valid then
      sink.get_transport_line(1).clear()
      sink.get_transport_line(2).clear()
    end
  end
end

return feed
