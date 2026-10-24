-- PT: Mod de validação. No primeiro tick monta uma superfície de laboratório, cola a blueprint
--     do teste, liga a energia e as entradas; depois espera o aquecimento, mede a produção e grava
--     o resultado em script-output/bf-validator/<id>.json. Usa só a API do jogo (sem io/os).
-- EN: Validation mod. On the first tick it builds a lab surface, pastes the test blueprint and
--     hooks up power and inputs; then it waits for the warm-up, measures production and writes
--     the result to script-output/bf-validator/<id>.json. It only uses the game API (no io/os).

local test = require("tests.current")
local setup = require("scripts.setup")
local measure = require("scripts.measure")
local feed = require("scripts.feed")

local SURFACE = "bf-lab"

-- PT: Grava o resultado e marca o teste como concluído.
-- EN: Writes the result and marks the test as finished.
local function finish(result)
  result.id = test and test.id or "unknown"
  result.game_tick = game.tick
  helpers.write_file("bf-validator/" .. result.id .. ".json", helpers.table_to_json(result), false)
  storage.phase = "done"
end

script.on_event(defines.events.on_tick, function()
  if storage.phase == "done" then
    return
  end
  if not test then
    finish({ ok = false, error = "no_test" })
    return
  end

  if storage.phase == nil then
    local ok, built = pcall(setup.run, test, SURFACE)
    if not ok then
      finish({ ok = false, error = "setup_failed", detail = tostring(built) })
      return
    end
    if not built.ok then
      finish(built)
      return
    end
    if test.auto_belts then
      local detected, found, reason =
        pcall(feed.detect, game.surfaces[SURFACE], game.forces.player, built.offset, test.drain_chests)
      if not detected or not found then
        finish({ ok = false, error = "feed_failed", detail = tostring(reason or found) })
        return
      end
      storage.feed = found
      built.belt_sources = #found.sources
      built.belt_sinks = #found.sinks
    end
    storage.built = built
    storage.start_tick = game.tick
    storage.phase = "warmup"
    return
  end

  if storage.feed then
    local fed, reason = pcall(feed.tick, storage.feed)
    if not fed then
      finish({ ok = false, error = "feed_failed", detail = tostring(reason) })
      return
    end
  end

  local elapsed = game.tick - storage.start_tick
  if storage.phase == "warmup" and elapsed >= test.warmup_ticks then
    storage.first = measure.snapshot(test, SURFACE)
    storage.first_tick = game.tick
    storage.phase = "measure"
  elseif storage.phase == "measure" and game.tick - storage.first_tick >= test.measure_ticks then
    local last = measure.snapshot(test, SURFACE)
    local result = measure.report(test, storage.first, last, game.tick - storage.first_tick)
    result.ok = true
    result.built = storage.built
    finish(result)
  end
end)
