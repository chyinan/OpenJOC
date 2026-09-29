// pattern: Imperative Shell
// Exercise the actual wasm32 ABI, including ownership, reset, drain and SOFA.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { createHash } from 'node:crypto';

const [wasmPath, mode, sofaPath] = process.argv.slice(2);
assert(wasmPath && ['embedded', 'external'].includes(mode) && sofaPath,
  'usage: node scripts/test-wasm.mjs MODULE.wasm embedded|external FIXTURE.sofa');
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {
  env: { openjoc_wasm_clock_now_ms: () => performance.now() },
});
const api = instance.exports;
const fixture = await readFile(new URL('../crates/openjoc-wasm/testdata/joc.ec3', import.meta.url));
const lifecycleFixture = await readFile(new URL('../crates/openjoc-wasm/testdata/joc.lifecycle.ec3', import.meta.url));
const sofa = await readFile(sofaPath);

function withBytes(bytes, allocator, action) {
  const pointer = allocator(bytes.length);
  assert(pointer !== 0, 'bounded allocation succeeds');
  try {
    new Uint8Array(api.memory.buffer, pointer, bytes.length).set(bytes);
    return action(pointer, bytes.length);
  } finally {
    api.openjoc_wasm_dealloc(pointer, bytes.length);
  }
}

function decode(handle, input, expectedUnits, expectedSamples) {
  const pcm = [];
  const frames = [];
  function collect() {
    for (let i = 0; i < 32; i++) {
      const status = api.openjoc_wasm_decoder_receive_pcm(handle);
      assert(status === 0 || status === 1, 'valid receive status');
      if (status === 0) return;
      const length = api.openjoc_wasm_decoder_pcm_len(handle);
      const samples = api.openjoc_wasm_decoder_pcm_samples(handle);
      assert.equal(length, samples * 2);
      assert.equal(api.openjoc_wasm_decoder_sample_rate(handle), 48000);
      assert.equal(api.openjoc_wasm_decoder_channel_count(handle), 2);
      const pointer = api.openjoc_wasm_decoder_pcm_ptr(handle);
      const values = new Float32Array(api.memory.buffer, pointer, length);
      assert(values.every(Number.isFinite));
      pcm.push(Buffer.from(new Uint8Array(api.memory.buffer, pointer, length * 4)));
      frames.push([samples, api.openjoc_wasm_decoder_pcm_pts_samples(handle)]);
      assert.equal(api.openjoc_wasm_decoder_consume_pcm(handle), 1);
    }
    assert.fail('PCM drain did not converge');
  }
  for (let offset = 0; offset < input.length; offset += 97) {
    const chunk = input.subarray(offset, offset + 97);
    const status = withBytes(chunk, api.openjoc_wasm_alloc,
      (pointer, length) => api.openjoc_wasm_decoder_push_bytes(handle, pointer, length));
    assert(status === 0 || status === 1, `push status ${status}`);
    collect();
  }
  let ended = false;
  for (let i = 0; i < 32; i++) {
    const status = api.openjoc_wasm_decoder_flush(handle);
    assert(status >= 0 && status <= 3, `flush status ${status}`);
    collect();
    if (status === 3) { ended = true; break; }
  }
  assert(ended, 'EOS reached within a bounded number of drains');
  assert.equal(api.openjoc_wasm_decoder_decoded_access_units(handle), expectedUnits);
  const bytes = Buffer.concat(pcm);
  assert(bytes.length > 0 && bytes.some(value => value !== 0), 'non-silent PCM');
  assert.equal(api.openjoc_wasm_decoder_output_samples(handle), BigInt(bytes.length / 8));
  assert.equal(bytes.length / 8, expectedSamples, 'complete programme plus exact FIR/gain tail');
  const mean = api.openjoc_wasm_decoder_total_mean_ms(handle);
  assert(Number.isFinite(mean) && mean >= 0);
  return { bytes, frames };
}

function checkDecoder(label, tailSamples, create) {
  const handle = create();
  assert(handle !== 0, `${label} constructor`);
  try {
    const first = decode(handle, fixture, 8, 1536 + tailSamples);
    assert.equal(api.openjoc_wasm_decoder_reset(handle), 0);
    assert.equal(api.openjoc_wasm_decoder_output_samples(handle), 0n);
    assert.equal(api.openjoc_wasm_decoder_total_mean_ms(handle), 0);
    const second = decode(handle, fixture, 8, 1536 + tailSamples);
    assert.deepEqual(second, first, `${label} reset preserves PCM bits and frame timing`);
    assert.equal(api.openjoc_wasm_decoder_reset(handle), 0);
    const continuous = decode(handle, lifecycleFixture, 128, 128 * 1536 + tailSamples);
    assert(continuous.frames.length > 100, 'continuous sequence emits multiple frames');
    console.log(`${label}: PASS, ${first.bytes.length / 8} samples, SHA256 ${createHash('sha256').update(first.bytes).digest('hex')}`);
  } finally {
    api.openjoc_wasm_decoder_destroy(handle);
  }
  assert.equal(api.openjoc_wasm_decoder_receive_pcm(handle), -1, 'destroyed handle rejected');
}

assert.equal(api.openjoc_wasm_decoder_receive_pcm(0), -1);
assert.equal(api.openjoc_wasm_custom_sofa_alloc(16 * 1024 * 1024 + 1), 0);
assert.equal(withBytes(Buffer.from('invalid SOFA'), api.openjoc_wasm_custom_sofa_alloc,
  (p, n) => api.openjoc_wasm_decoder_create_with_renderer_and_custom_sofa(0, 1, p, n)), 0);
checkDecoder('Stereo', 32, () => api.openjoc_wasm_decoder_create());
for (const [index, name] of ['sadie-ii-d1-ku100', 'sadie-ii-d2-kemar'].entries()) {
  const asset = await readFile(new URL(`../crates/openjoc-sofa/assets/${name}.ojhrtf`, import.meta.url));
  checkDecoder(name, 255, () => mode === 'embedded'
    ? api.openjoc_wasm_decoder_create_with_renderer_and_hrtf(0, 1, index)
    : withBytes(asset, api.openjoc_wasm_hrtf_asset_alloc,
      (p, n) => api.openjoc_wasm_decoder_create_with_renderer_and_hrtf_asset(0, 1, index, p, n)));
}
checkDecoder('Custom SOFA', 1, () => withBytes(sofa, api.openjoc_wasm_custom_sofa_alloc,
  (p, n) => api.openjoc_wasm_decoder_create_with_renderer_and_custom_sofa(0, 1, p, n)));
