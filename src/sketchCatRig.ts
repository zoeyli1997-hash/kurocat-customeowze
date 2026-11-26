import { validateSkin, DEFAULT_RIG, PART_NAMES, type SketchSkinPackage, type RigSettings } from './customSkin';

type Point = { x: number; y: number };

/** Renders the approved hand-drawn artwork without replacing the live input engine. */
export class SketchCatRig {
  private parts: HTMLCanvasElement[] = [];
  private onReady: () => void;
  private generation = 0;
  private rig: RigSettings = { ...DEFAULT_RIG };
  public skinId: string | null = null;

  constructor(onReady: () => void) { this.onReady = onReady; }

  public async load(input: SketchSkinPackage): Promise<void> {
    const skin = validateSkin(input);
    const generation = ++this.generation;
    this.skinId = null;
    const image = new Image();
    await new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error('无法加载小猫动作素材。'));
      image.src = skin.atlasDataUrl;
    });
    const parts: HTMLCanvasElement[] = [];
    for (const key of PART_NAMES) {
      const crop = skin.parts[key];
      const x = Math.floor(crop.x * image.width), y = Math.floor(crop.y * image.height);
      const width = Math.max(1, Math.min(image.width - x, Math.round(crop.w * image.width)));
      const height = Math.max(1, Math.min(image.height - y, Math.round(crop.h * image.height)));
      const cell = document.createElement('canvas');
      cell.width = width; cell.height = height;
      const ctx = cell.getContext('2d')!;
      ctx.drawImage(image, x, y, width, height, 0, 0, width, height);
      const rgba = ctx.getImageData(0, 0, width, height).data;
      let x0 = width, y0 = height, x1 = 0, y1 = 0;
      for (let row = 0; row < height; row++) for (let col = 0; col < width; col++) {
        if (rgba[(row * width + col) * 4 + 3] < 64) continue;
        x0 = Math.min(x0, col); y0 = Math.min(y0, row);
        x1 = Math.max(x1, col); y1 = Math.max(y1, row);
      }
      if (x0 > x1 || y0 > y1) throw new Error(`${key} 部件为空。`);
      const part = document.createElement('canvas');
      part.width = x1 - x0 + 1; part.height = y1 - y0 + 1;
      part.getContext('2d')!.drawImage(cell, x0, y0, part.width, part.height, 0, 0, part.width, part.height);
      parts.push(part);
    }
    if (generation !== this.generation) return;
    this.parts = parts; this.rig = { ...skin.rig }; this.skinId = skin.id; this.onReady();
  }

  get ready(): boolean { return this.parts.length === 6; }

  body(ctx: CanvasRenderingContext2D, center: Point, base: Point, breath: number): void {
    if (!this.ready) return;
    ctx.save();
    ctx.translate(center.x, base.y);
    ctx.rotate(Math.max(-0.08, Math.min(0.08, (center.x - base.x) * -0.002)));
    const scale = this.rig.bodyScale;
    ctx.drawImage(this.parts[2], -70 * scale, -133 * scale - breath * 0.35, 140 * scale, 135 * scale + breath * 0.35);
    ctx.restore();
  }

  head(ctx: CanvasRenderingContext2D, center: Point, tilt: number, closed: number): void {
    if (!this.ready) return;
    ctx.save();
    ctx.translate(center.x + this.rig.headOffsetX, center.y + this.rig.headOffsetY);
    ctx.rotate(tilt);
    // The open and closed atlas heads share the same silhouette and attachment point.
    const scale = this.rig.headScale;
    ctx.drawImage(this.parts[closed > 0.52 ? 1 : 0], -82 * scale, -67 * scale, 164 * scale, 139 * scale);
    ctx.restore();
  }

  arm(ctx: CanvasRenderingContext2D, side: 'left' | 'right', start: Point, end: Point, squash: number): void {
    if (!this.ready) return;
    const part = this.parts[side === 'left' ? 4 : 5];
    const length = Math.max(30, Math.hypot(end.x - start.x, end.y - start.y));
    const width = 30 * this.rig.pawScale * (1 + squash * 0.08);
    const split = Math.round(part.height * 0.81);
    const pawHeight = 19 * (1 - squash * 0.12);
    ctx.save();
    ctx.translate(start.x, start.y);
    ctx.rotate(Math.atan2(end.y - start.y, end.x - start.x) - Math.PI / 2);
    // Stretch the forearm, retaining the drawn paw's proportions at the contact point.
    ctx.drawImage(part, 0, 0, part.width, split, -width / 2, -2, width, length - pawHeight / 2 + 2);
    ctx.drawImage(part, 0, split, part.width, part.height - split,
      -width / 2, length - pawHeight / 2, width, pawHeight);
    ctx.restore();
  }

  tail(ctx: CanvasRenderingContext2D, base: Point, tip: Point, reach: number, time: number): void {
    if (!this.ready) return;
    const rest = { x: base.x + 57, y: base.y - 11 + Math.sin(time * 1.5) * 2 };
    const end = { x: rest.x + (tip.x - rest.x) * reach, y: rest.y + (tip.y - rest.y) * reach };
    const dx = end.x - base.x, dy = end.y - base.y;
    ctx.save();
    ctx.translate(base.x, base.y);
    ctx.rotate(Math.atan2(dy, dx));
    ctx.drawImage(this.parts[3], 0, -29, Math.max(44, Math.hypot(dx, dy)), 42);
    ctx.restore();
  }
}
