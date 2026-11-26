export const PART_NAMES = ['headOpen', 'headClosed', 'body', 'tail', 'leftArm', 'rightArm'] as const;
export type PartName = typeof PART_NAMES[number];
export interface Crop { x: number; y: number; w: number; h: number }
export interface RigSettings { headScale: number; bodyScale: number; headOffsetX: number; headOffsetY: number; pawScale: number }
export interface SketchSkinPackage {
  version: 1;
  style: 'loose-crayon-v1';
  id: string;
  name: string;
  createdAt: string;
  atlasDataUrl: string;
  parts: Record<PartName, Crop>;
  rig: RigSettings;
}
export const DEFAULT_RIG: RigSettings = { headScale: 1, bodyScale: 1, headOffsetX: 0, headOffsetY: 0, pawScale: 1 };
export const MAX_PACKAGE_BYTES = 20 * 1024 * 1024;

export function gridParts(): Record<PartName, Crop> {
  return Object.fromEntries(PART_NAMES.map((key, i) => [key, { x: (i % 2) / 2, y: Math.floor(i / 2) / 3, w: 0.5, h: 1 / 3 }])) as Record<PartName, Crop>;
}
export function createSkin(name: string, atlasDataUrl: string): SketchSkinPackage {
  return validateSkin({ version: 1, style: 'loose-crayon-v1', id: `diy-${crypto.randomUUID().slice(0, 24)}`,
    name, createdAt: new Date().toISOString(), atlasDataUrl, parts: gridParts(), rig: { ...DEFAULT_RIG } });
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw Error('皮肤包格式不正确。');
  return value as Record<string, unknown>;
}
function numeric(value: unknown, min: number, max: number, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) throw Error(`${label}超出允许范围。`);
  return value;
}
export function validateSkin(value: unknown): SketchSkinPackage {
  const raw = object(value);
  if (raw.version !== 1 || raw.style !== 'loose-crayon-v1') throw Error('不支持这个皮肤包版本或画风。');
  if (typeof raw.id !== 'string' || !/^diy-[a-z0-9_-]{1,32}$/.test(raw.id)) throw Error('小猫编号无效。');
  if (typeof raw.name !== 'string' || !raw.name.trim() || raw.name.length > 60) throw Error('请填写 1–60 字的小猫名称。');
  if (typeof raw.createdAt !== 'string' || raw.createdAt.length > 40 || !Number.isFinite(Date.parse(raw.createdAt))) throw Error('皮肤包日期无效。');
  if (typeof raw.atlasDataUrl !== 'string' || raw.atlasDataUrl.length > 14 * 1024 * 1024 || !/^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(raw.atlasDataUrl)) throw Error('动作素材必须是 10 MB 内的 PNG 图片。');
  let header: string;
  try { header = atob(raw.atlasDataUrl.split(',')[1].slice(0, 48)); } catch { throw Error('PNG 图片编码无效。'); }
  const expected = [137,80,78,71,13,10,26,10];
  if (header.length < 24 || expected.some((byte,i)=>header.charCodeAt(i)!==byte)) throw Error('素材不是有效的 PNG。');
  const u32 = (at: number) => ((header.charCodeAt(at) * 0x1000000) + (header.charCodeAt(at+1)<<16) + (header.charCodeAt(at+2)<<8) + header.charCodeAt(at+3));
  if (u32(16) < 64 || u32(20) < 64 || u32(16) > 4096 || u32(20) > 4096) throw Error('动作素材尺寸应在 64–4096 像素之间。');
  const rawParts = object(raw.parts);
  const parts = {} as Record<PartName, Crop>;
  for (const key of PART_NAMES) {
    const p = object(rawParts[key]);
    const crop = { x: numeric(p.x,0,1,key), y: numeric(p.y,0,1,key), w: numeric(p.w,0.01,1,key), h: numeric(p.h,0.01,1,key) };
    if (crop.x + crop.w > 1.000001 || crop.y + crop.h > 1.000001) throw Error(`${key} 部件超出图片边界。`);
    parts[key] = crop;
  }
  const r = object(raw.rig);
  const rig = { headScale: numeric(r.headScale,0.65,1.4,'头部大小'), bodyScale: numeric(r.bodyScale,0.65,1.4,'身体大小'),
    headOffsetX: numeric(r.headOffsetX,-35,35,'头部横移'), headOffsetY: numeric(r.headOffsetY,-35,35,'头部纵移'), pawScale: numeric(r.pawScale,0.65,1.4,'爪子大小') };
  // A canonical whitelist means source photos, API credentials and arbitrary properties cannot be exported.
  return { version: 1, style: 'loose-crayon-v1', id: raw.id, name: raw.name.trim(), createdAt: raw.createdAt,
    atlasDataUrl: raw.atlasDataUrl, parts, rig };
}
export function exportSkin(skin: unknown): string { return JSON.stringify(validateSkin(skin), null, 2); }
export function parseSkin(text: string): SketchSkinPackage {
  if (new TextEncoder().encode(text).length > MAX_PACKAGE_BYTES) throw Error('皮肤包超过 20 MB。');
  try { return validateSkin(JSON.parse(text)); } catch (e) { if (e instanceof SyntaxError) throw Error('皮肤包不是有效的 JSON 文件。'); throw e; }
}
export async function validateAtlasInBrowser(skin: SketchSkinPackage): Promise<void> {
  const img = new Image();
  await new Promise<void>((resolve,reject)=>{img.onload=()=>resolve(); img.onerror=()=>reject(Error('无法解码动作图片。'));img.src=skin.atlasDataUrl;});
  const canvas = document.createElement('canvas'); canvas.width=img.width;canvas.height=img.height;
  const ctx=canvas.getContext('2d')!;ctx.drawImage(img,0,0);
  for(const name of PART_NAMES){
    const r=skin.parts[name];const w=Math.max(1,Math.floor(r.w*img.width)),h=Math.max(1,Math.floor(r.h*img.height));
    const data=ctx.getImageData(Math.floor(r.x*img.width),Math.floor(r.y*img.height),w,h).data;
    let opaque=0,transparent=0;
    for(let i=3;i<data.length;i+=4){if(data[i]>96)opaque++;if(data[i]<16)transparent++;}
    if(opaque < w*h*0.006)throw Error(`${name} 部件为空，请调整裁切范围或重新生成。`);
    if(transparent < w*h*0.015)throw Error(`${name} 缺少透明背景，请使用透明 PNG 动作素材。`);
  }
}
