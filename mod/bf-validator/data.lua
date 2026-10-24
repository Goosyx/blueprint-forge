-- PT: Poste usado só pelo modo de energia `cover` dos testes: alcance máximo, sem colisão e
--     invisível no mapa. Não muda nenhum protótipo do jogo base.
-- EN: Pole used only by the tests' `cover` power mode: maximum reach, no collision and hidden
--     on the map. It changes no base-game prototype.
local pole = table.deepcopy(data.raw["electric-pole"]["substation"])
pole.name = "bf-validator-pole"
pole.supply_area_distance = 64
pole.maximum_wire_distance = 64
pole.collision_box = { { -0.1, -0.1 }, { 0.1, 0.1 } }
pole.selection_box = { { -0.1, -0.1 }, { 0.1, 0.1 } }
pole.collision_mask = { layers = {} }
pole.flags = { "placeable-off-grid", "not-on-map", "not-blueprintable", "not-deconstructable" }
pole.minable = nil
pole.hidden = true
pole.next_upgrade = nil
pole.fast_replaceable_group = nil
data:extend({ pole })
