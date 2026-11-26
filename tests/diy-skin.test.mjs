import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createSkin, validateSkin, parseSkin, exportSkin } from '../src/customSkin.ts';

const atlas = JSON.parse(readFileSync(new URL('../examples/golden-cat.catpack', import.meta.url), 'utf8')).atlasDataUrl;
test('a skin round trips with six components and calibration, without source photos or keys', () => {
  const skin = createSkin('小煤球', atlas);
  skin.rig.headScale = 1.07;
  const saved = parseSkin(exportSkin(skin));
  assert.equal(saved.name, '小煤球');
  assert.equal(saved.rig.headScale, 1.07);
  assert.equal(Object.keys(saved.parts).length, 6);
  assert.equal(saved.atlasDataUrl, atlas);
  assert.ok(!exportSkin({...skin, apiKey:'secret', photos:['private']}).includes('secret'));
  assert.ok(!exportSkin({...skin, apiKey:'secret', photos:['private']}).includes('private'));
});
test('unsupported versions, remote images, path IDs and out-of-bounds crops are rejected', () => {
  const skin = createSkin('三花', atlas);
  for (const bad of [{...skin,version:99},{...skin,id:'../../oops'},{...skin,atlasDataUrl:'https://example.com/cat.png'},
    {...skin,parts:{...skin.parts,headOpen:{x:0.9,y:0,w:0.5,h:0.3}}}]) {
    assert.throws(()=>validateSkin(bad));
  }
});
test('coat is not part of the fixed style and photos remain the identity source', () => {
  const prompt=readFileSync(new URL('../docs/atlas-prompt.txt', import.meta.url),'utf8');
  assert.ok(prompt.includes('Coat colors, markings, eye colors'));
  assert.ok(prompt.includes('only from the uploaded photos'));
  assert.ok(!prompt.includes('honey-gold'));
  assert.ok(!prompt.includes('grey-tipped'));
  assert.ok(prompt.includes('No clothing or accessories'));
});
