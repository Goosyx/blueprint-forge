// PT: Desenho da blueprint com PixiJS (WebGL). Cada entidade vira um retângulo do tamanho da área
//     ocupada, com o ícone real no centro e uma seta de direção. Pan com arrastar, zoom com a roda,
//     e só o que está na tela é desenhado (culling).
// EN: Blueprint drawing with PixiJS (WebGL). Each entity becomes a rectangle the size of its
//     footprint, with the real icon in the center and a direction arrow. Drag to pan, wheel to
//     zoom, and only what is on screen is drawn (culling).
import {
  Application,
  Assets,
  Container,
  CullerPlugin,
  Graphics,
  Rectangle,
  Sprite,
  Texture,
  extensions,
} from 'pixi.js';
// PT: Troca a geração de código com `eval` do Pixi por versões que respeitam a CSP sem 'unsafe-eval'.
// EN: Replaces Pixi's `eval`-based code generation with versions that respect a CSP without 'unsafe-eval'.
import 'pixi.js/unsafe-eval';
import type { Atlas } from '../game/assets';
import type { PlacedEntity, World } from '../types';
import { arrowDirection, categoryColor, isDirectional } from './style';

extensions.add(CullerPlugin);

/** PT: Pixels por tile no zoom 1. EN: Pixels per tile at zoom 1. */
const TILE = 32;
const MIN_ZOOM = 0.02;
const MAX_ZOOM = 8;
/** PT: Lado do bloco do índice de hover, em tiles. EN: Hover index block side, in tiles. */
const BLOCK = 32;

export interface HoverInfo {
  entity: PlacedEntity | null;
  screenX: number;
  screenY: number;
}

export class BlueprintScene {
  private app = new Application();
  private world = new Container();
  private shapes = new Graphics();
  private arrows = new Graphics();
  private icons = new Container();
  private textures: Texture[] = [];
  private atlas: Atlas | null = null;
  private iconCache = new Map<string, Texture>();
  /** PT: Bloco de 32×32 tiles → entidades que o tocam. EN: 32×32 tile block → entities touching it. */
  private blocks = new Map<string, PlacedEntity[]>();
  private current: World | null = null;
  private dragging: { x: number; y: number } | null = null;
  private ready = false;
  private resizeObserver: ResizeObserver | null = null;

  constructor(
    private host: HTMLElement,
    private onHover: (info: HoverInfo) => void,
  ) {}

  /** PT: Cria o canvas WebGL dentro do elemento. EN: Creates the WebGL canvas inside the element. */
  async init(): Promise<void> {
    await this.app.init({
      resizeTo: this.host,
      background: '#16171b',
      antialias: true,
      autoDensity: true,
      resolution: globalThis.devicePixelRatio || 1,
    });
    this.host.appendChild(this.app.canvas);
    this.world.addChild(this.shapes, this.icons, this.arrows);
    this.app.stage.addChild(this.world);
    this.bindInput();
    // PT: Acompanha mudanças de tamanho do elemento, não só da janela.
    // EN: Follows size changes of the element, not just of the window.
    this.resizeObserver = new ResizeObserver(() => this.app.resize());
    this.resizeObserver.observe(this.host);
    this.ready = true;
  }

  /** PT: Carrega as páginas do atlas de ícones. EN: Loads the icon atlas pages. */
  async setAtlas(atlas: Atlas, pageUrl: (page: string) => string): Promise<void> {
    // PT: Sem workers criados a partir de blob, para manter a CSP `worker-src 'self'`.
    // EN: No blob-created workers, to keep the `worker-src 'self'` CSP.
    Assets.setPreferences({ preferWorkers: false });
    const pages = await Promise.all(
      atlas.pages.map((page) => Assets.load<Texture>({ src: pageUrl(page), parser: 'texture' })),
    );
    this.textures = pages;
    this.atlas = atlas;
    this.iconCache.clear();
    if (this.current) this.show(this.current);
  }

  /** PT: Desenha um mundo e enquadra a vista. EN: Draws a world and fits the view. */
  show(world: World): void {
    if (!this.ready) return;
    this.current = world;
    this.shapes.clear();
    this.arrows.clear();
    for (const child of this.icons.removeChildren()) child.destroy();
    this.blocks.clear();

    for (const entity of world.entities) {
      const { x, y, width, height } = entity.footprint;
      const color = categoryColor(entity.prototype);
      const inset = 0.06 * TILE;
      this.shapes
        .roundRect(
          x * TILE + inset,
          y * TILE + inset,
          width * TILE - 2 * inset,
          height * TILE - 2 * inset,
          3,
        )
        .fill({ color, alpha: 0.55 })
        .stroke({ width: 1, color, alpha: 0.95 });

      this.index(entity);

      const icon = this.iconFor(entity);
      if (icon) {
        const sprite = new Sprite(icon);
        const size = Math.min(width, height) * TILE * (entity.recipe ? 0.7 : 0.8);
        sprite.width = size;
        sprite.height = size;
        sprite.anchor.set(0.5);
        sprite.position.set((x + width / 2) * TILE, (y + height / 2) * TILE);
        sprite.cullable = true;
        this.icons.addChild(sprite);
      }

      if (isDirectional(entity.prototype)) this.drawArrow(entity);
    }
    this.fit();
  }

  /** PT: Enquadra a blueprint inteira na tela. EN: Fits the whole blueprint on screen. */
  fit(): void {
    const bounds = this.current?.bounds;
    if (!bounds) return;
    const width = (bounds.max_x - bounds.min_x) * TILE;
    const height = (bounds.max_y - bounds.min_y) * TILE;
    const screen = this.app.screen;
    const zoom = clamp(
      Math.min(screen.width / (width + 2 * TILE), screen.height / (height + 2 * TILE)),
      MIN_ZOOM,
      MAX_ZOOM,
    );
    this.world.scale.set(zoom);
    this.world.position.set(
      screen.width / 2 - ((bounds.min_x + bounds.max_x) / 2) * TILE * zoom,
      screen.height / 2 - ((bounds.min_y + bounds.max_y) / 2) * TILE * zoom,
    );
  }

  /** PT: Libera o WebGL e os recursos. EN: Releases WebGL and resources. */
  destroy(): void {
    this.ready = false;
    this.resizeObserver?.disconnect();
    this.app.destroy(true, { children: true });
  }

  // PT: Ícone da receita (se houver) ou da entidade, recortado do atlas.
  // EN: Recipe icon (if any) or entity icon, cut from the atlas.
  private iconFor(entity: PlacedEntity): Texture | null {
    if (!this.atlas) return null;
    const keys = entity.recipe ? [`recipe/${entity.recipe}`, `item/${entity.recipe}`] : [];
    keys.push(`entity/${entity.name}`, `item/${entity.name}`);
    for (const key of keys) {
      const cached = this.iconCache.get(key);
      if (cached) return cached;
      const slot = this.atlas.icons[key];
      const page = slot ? this.textures[slot.page] : undefined;
      if (slot && page) {
        const cell = this.atlas.cell;
        const texture = new Texture({
          source: page.source,
          frame: new Rectangle(slot.col * cell, slot.row * cell, cell, cell),
        });
        this.iconCache.set(key, texture);
        return texture;
      }
    }
    return null;
  }

  // PT: Coloca a entidade em cada bloco de 32×32 que ela toca (memória proporcional às entidades).
  // EN: Puts the entity into each 32×32 block it touches (memory proportional to entities).
  private index(entity: PlacedEntity): void {
    const { x, y, width, height } = entity.footprint;
    const x1 = Math.floor(x / BLOCK);
    const y1 = Math.floor(y / BLOCK);
    const x2 = Math.floor((x + width - 1) / BLOCK);
    const y2 = Math.floor((y + height - 1) / BLOCK);
    for (let bx = x1; bx <= x2; bx++) {
      for (let by = y1; by <= y2; by++) {
        const key = `${bx},${by}`;
        const list = this.blocks.get(key);
        if (list) list.push(entity);
        else this.blocks.set(key, [entity]);
      }
    }
  }

  private drawArrow(entity: PlacedEntity): void {
    const { x, y, width, height } = entity.footprint;
    const angle = (arrowDirection(entity.prototype, entity.direction) * Math.PI) / 8;
    const cx = (x + width / 2) * TILE;
    const cy = (y + height / 2) * TILE;
    const length = Math.min(width, height) * TILE * 0.3;
    const tip = rotate(0, -length, angle);
    const left = rotate(-length * 0.6, length * 0.2, angle);
    const right = rotate(length * 0.6, length * 0.2, angle);
    this.arrows
      .poly([cx + tip.x, cy + tip.y, cx + left.x, cy + left.y, cx + right.x, cy + right.y])
      .fill({ color: 0xffffff, alpha: 0.85 });
  }

  private bindInput(): void {
    const canvas = this.app.canvas;
    canvas.addEventListener('pointerdown', (event) => {
      this.dragging = { x: event.clientX, y: event.clientY };
      canvas.setPointerCapture(event.pointerId);
    });
    canvas.addEventListener('pointerup', (event) => {
      this.dragging = null;
      canvas.releasePointerCapture(event.pointerId);
    });
    canvas.addEventListener('pointermove', (event) => {
      if (this.dragging) {
        this.world.position.x += event.clientX - this.dragging.x;
        this.world.position.y += event.clientY - this.dragging.y;
        this.dragging = { x: event.clientX, y: event.clientY };
      }
      this.hover(event);
    });
    canvas.addEventListener('pointerleave', () => {
      this.onHover({ entity: null, screenX: 0, screenY: 0 });
    });
    canvas.addEventListener(
      'wheel',
      (event) => {
        event.preventDefault();
        const rect = canvas.getBoundingClientRect();
        const px = event.clientX - rect.left;
        const py = event.clientY - rect.top;
        const old = this.world.scale.x;
        const zoom = clamp(old * Math.exp(-event.deltaY * 0.0015), MIN_ZOOM, MAX_ZOOM);
        // PT: Mantém o ponto sob o cursor parado durante o zoom.
        // EN: Keeps the point under the cursor still while zooming.
        this.world.position.x = px - ((px - this.world.position.x) * zoom) / old;
        this.world.position.y = py - ((py - this.world.position.y) * zoom) / old;
        this.world.scale.set(zoom);
      },
      { passive: false },
    );
  }

  private hover(event: PointerEvent): void {
    const rect = this.app.canvas.getBoundingClientRect();
    const px = event.clientX - rect.left;
    const py = event.clientY - rect.top;
    const zoom = this.world.scale.x;
    const tileX = Math.floor((px - this.world.position.x) / zoom / TILE);
    const tileY = Math.floor((py - this.world.position.y) / zoom / TILE);
    const candidates = this.blocks.get(blockKey(tileX, tileY)) ?? [];
    const entity =
      candidates.find(({ footprint: f }) => {
        return tileX >= f.x && tileX < f.x + f.width && tileY >= f.y && tileY < f.y + f.height;
      }) ?? null;
    this.onHover({ entity, screenX: px, screenY: py });
  }
}

function blockKey(tileX: number, tileY: number): string {
  return `${Math.floor(tileX / BLOCK)},${Math.floor(tileY / BLOCK)}`;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function rotate(x: number, y: number, angle: number): { x: number; y: number } {
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return { x: x * cos - y * sin, y: x * sin + y * cos };
}
