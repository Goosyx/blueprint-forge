# Simulação de fluxo: relatório / Flow simulation: report

Factorio 2.1.23, servidor headless oficial / official headless server.

## Precisão contra o jogo / Accuracy against the game

Detalhes por cenário / Per-scenario details: [`sim-comparison.md`](sim-comparison.md) (`bf-validate compare-sim`).

| Versão / Version | Erro médio / Mean error | Sem travamento / Without deadlock | Tempo típico / Typical time |
|---|---:|---:|---:|
| Regime estável / Steady state | 13,33% | 0,00% | 0,05–2 ms |
| Tick a tick / Tick by tick | 0,94% | 1,12% | 20–250 ms |

- O regime estável erra só o cenário `flow-mixed-lane`, um travamento real do jogo (ferro e cobre na mesma faixa param a esteira). Por construção, um ponto fixo de vazões não prevê travamento por posição dos itens.
  The steady state only misses `flow-mixed-lane`, a real game deadlock (iron and copper on the same lane stop the belt). By construction, a throughput fixed point cannot predict an item-position deadlock.
- A simulação tick a tick acerta o travamento; o maior erro dela é `flow-inserter-limited` (11%): pegar 3 itens de uma esteira em movimento ainda não está calibrado.
  The tick simulation gets the deadlock right; its largest error is `flow-inserter-limited` (11%): picking 3 items from a moving belt is not fully calibrated yet.

Vazão dos insersores medida no jogo / Inserter throughput measured in-game: [`inserter-throughput.json`](inserter-throughput.json) (66 casos / cases). A simulação tick a tick reproduz a tabela com erro médio de 3,4% (`bf-validate calibrate-tick`).
The tick simulation reproduces the table with a 3.4% mean error (`bf-validate calibrate-tick`).

## Custo / Cost

Blueprints sintéticas de linhas de montadoras (`bf-validate bench-sim`); tick: 600 + 3600 ticks.
Synthetic blueprints of assembler rows (`bf-validate bench-sim`); tick: 600 + 3600 ticks.

| Entidades / Entities | Análise + regime estável, nativo / Analysis + steady, native | Tick, nativo / native | Análise + regime estável, WASM / Analysis + steady, WASM | Tick, WASM |
|---:|---:|---:|---:|---:|
| ~100 | 0,3 ms | 3 ms | 1,7 ms | 6 ms |
| ~1.000 | 3,1 ms | 29 ms | 7,2 ms | 49 ms |
| ~10.000 | 33 ms | 313 ms | 60 ms | 480 ms |

Pico de memória com ~9.200 entidades (leitura, cálculo e as duas simulações, `dhat`): **11,7 MB**.
Peak memory with ~9,200 entities (reading, calculation and both simulations, `dhat`): **11.7 MB**.

Blueprints hostis do code review (marcadores e postes empilhados, linha de fornalhas, pente de entradas laterais, escada de splitters, 60 mil esteiras com 120 receitas) ficam entre 12 e 125 ms no nativo (`crates/bf-analysis/tests/hostile_flow.rs`). A simulação estável tem teto de células (2,5 milhões) e de trabalho.
Hostile blueprints from the code review (stacked markers and poles, furnace line, sideload comb, splitter staircase, 60 thousand belts with 120 recipes) stay between 12 and 125 ms natively (`crates/bf-analysis/tests/hostile_flow.rs`). The steady simulation has a cell cap (2.5 million) and a work cap.

Com o padrão da interface (10 min de aquecimento + 2 min de medição = 43.200 ticks), a simulação tick a tick leva cerca de 10× o tempo da tabela.
With the interface default (10 min warm-up + 2 min measurement = 43,200 ticks), the tick simulation takes about 10× the table's time.

## Decisão / Decision

Critério: se a diferença de erro médio for até 1 ponto percentual, fica a de menor custo. A diferença é de 12,4 pontos (por causa do travamento); sem o travamento, o regime estável é mais preciso e de 30 a 100 vezes mais barato.
Criterion: if the mean error difference is at most 1 percentage point, the cheaper one stays. The difference is 12.4 points (because of the deadlock); without the deadlock, the steady state is more accurate and 30 to 100 times cheaper.

Ficam as duas: o regime estável em toda análise (gargalos, sankey, cores de uso) e a tick a tick sob demanda, para achar travamentos.
Both stay: the steady state on every analysis (bottlenecks, sankey, usage colors) and the tick one on demand, to find deadlocks.
