import test from 'node:test';
import assert from 'node:assert/strict';

class FakeAudioContext {
  static instances = [];
  state = 'suspended';
  currentTime = 1;
  sampleRate = 8000;
  destination = {};
  gains = [];
  constructor(options) { this.options = options; FakeAudioContext.instances.push(this); }
  resume() { this.state = 'running'; return Promise.resolve(); }
  createBuffer(channels, length) { return { getChannelData: () => new Float32Array(length) }; }
  node() { return { connect() {}, start() {} }; }
  createBufferSource() { return this.node(); }
  createBiquadFilter() { return { ...this.node(), frequency: { value: 0 }, Q: { value: 0 } }; }
  createOscillator() { return { ...this.node(), frequency: { value: 0 } }; }
  createGain() {
    const gain = { value: 0, targets: [], setTargetAtTime(value, time, constant) { this.targets.push({value, time, constant}); } };
    const node = { ...this.node(), gain };
    this.gains.push(node);
    return node;
  }
}
globalThis.window = { AudioContext: FakeAudioContext };

async function fresh(name) {
  FakeAudioContext.instances.length = 0;
  return (await import(`../src/audio.ts?${name}`)).catAudio;
}

test('enabling sound silently prepares the graph before the first click', async () => {
  const audio = await fresh('prewarm');
  audio.setEnabled(true);
  assert.equal(FakeAudioContext.instances.length, 1, 'audio graph should already exist');
  const ctx = FakeAudioContext.instances[0];
  assert.equal(ctx.options?.latencyHint, 'interactive');
  const master = ctx.gains.at(-1).gain;
  assert.equal(master.value, 0, 'warming audio must remain silent');
  assert.equal(master.targets.length, 0);
});

test('purr attack reaches its target quickly instead of taking hundreds of milliseconds', async () => {
  const audio = await fresh('attack');
  audio.setPurrIntensity(0.8);
  const target = FakeAudioContext.instances[0].gains.at(-1).gain.targets.at(-1);
  assert.ok(target.value > 0);
  assert.ok(target.constant <= 0.025, `attack constant ${target.constant} is too slow`);
});

test('muting never starts a new audio context', async () => {
  const audio = await fresh('muted');
  audio.setEnabled(false);
  audio.setPurrIntensity(1);
  assert.equal(FakeAudioContext.instances.length, 0);
});
