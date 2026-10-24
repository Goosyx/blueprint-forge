# blueprint-forge

**[Português](#português) · [English](#english)**

---

## Português

Ferramenta web para blueprints do **Factorio 2.1**, com duas funções:

- **Interpretar** uma blueprint: produção e consumo com módulos e beacons, energia, poluição, lista de materiais, fluxo de esteiras e insersores, gargalos e gráfico sankey.
- **Gerar** uma blueprint completa e tileável para produzir um item numa taxa pedida, com robôs ou esteiras, pronta para colar no jogo.

> Em desenvolvimento.

### Estrutura

| Pasta | Conteúdo |
|---|---|
| `crates/` | Núcleo em Rust: dados do jogo, codec de blueprint, cálculo, simulação, gerador, WASM e servidor. |
| `tools/` | Ferramentas de linha de comando: exportação dos dados do jogo e validação dentro do jogo. |
| `web/` | Interface em Svelte + TypeScript. |
| `scripts/` | Instalação das ferramentas, diagnóstico do ambiente e CI local. |

### Dados do jogo

Os dados, textos e ícones do Factorio pertencem à Wube Software e **não fazem parte deste repositório**. Eles são gerados na sua máquina, a partir da sua cópia do jogo, em `game-assets/`.

### Requisitos

Rode `scripts/doctor.sh` para conferir o ambiente. Ele mostra o que falta e como instalar.

### Como rodar

```sh
scripts/install-tools.sh            # Rust, Node, gh, wasm-opt (sha256 conferido)
cp .env.example .env                # troque as senhas
docker compose up -d db             # Postgres local (só em 127.0.0.1)
scripts/db-setup.sh                 # usuário da aplicação + migrations
cargo run --release -p bf-dump -- export   # dados, nomes e ícones da sua cópia do jogo
scripts/dev.sh serve                # compila tudo e abre em http://localhost:8080
```

O cadastro vem fechado. Para criar a sua conta, rode `scripts/dev.sh create-user voce@exemplo.com` e digite a senha.

Validação dentro do jogo (servidor headless oficial, gratuito): `scripts/fetch-headless.sh` e depois `cargo run --release -p bf-validate -- run validation/scenarios/<cenário>.json`.

### Licença

MIT ou Apache-2.0, à sua escolha. Veja [`LICENSE-MIT`](LICENSE-MIT) e [`LICENSE-APACHE`](LICENSE-APACHE).

---

## English

Web tool for **Factorio 2.1** blueprints, with two functions:

- **Analyze** a blueprint: production and consumption with modules and beacons, power, pollution, bill of materials, belt and inserter flow, bottlenecks and a sankey chart.
- **Generate** a complete, tileable blueprint that produces an item at a requested rate, using robots or belts, ready to paste into the game.

> Work in progress.

### Layout

| Folder | Contents |
|---|---|
| `crates/` | Rust core: game data, blueprint codec, calculator, simulation, generator, WASM and server. |
| `tools/` | Command-line tools: game data export and in-game validation. |
| `web/` | Svelte + TypeScript user interface. |
| `scripts/` | Tool installation, environment diagnostics and local CI. |

### Game data

Factorio's data, text and icons belong to Wube Software and are **not part of this repository**. They are generated on your machine, from your own copy of the game, into `game-assets/`.

### Requirements

Run `scripts/doctor.sh` to check your environment. It reports what is missing and how to install it.

### Running

```sh
scripts/install-tools.sh            # Rust, Node, gh, wasm-opt (sha256 checked)
cp .env.example .env                # change the passwords
docker compose up -d db             # local Postgres (127.0.0.1 only)
scripts/db-setup.sh                 # application user + migrations
cargo run --release -p bf-dump -- export   # data, names and icons from your copy of the game
scripts/dev.sh serve                # builds everything and serves http://localhost:8080
```

Registration is closed by default. To create your account, run `scripts/dev.sh create-user you@example.com` and type the password.

In-game validation (official, free headless server): `scripts/fetch-headless.sh`, then `cargo run --release -p bf-validate -- run validation/scenarios/<scenario>.json`.

### License

MIT or Apache-2.0, at your option. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
