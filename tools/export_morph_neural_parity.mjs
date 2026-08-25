// Produces a small golden fixture from the colleague's original JavaScript
// LIF engine. The Rust port runs the identical current schedule in its tests.

import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
const arg = name => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : null;
};
const root = path.resolve(arg('--root') || '_external/morph');
const output = path.resolve(arg('--output') || 'crates/morph_brain/assets/neural-parity.json');
const { buildBrain } = await import(pathToFileURL(path.join(root, 'web/src/brain/spec.js')).href);
const net = buildBrain(1234);
const drive = (name, value) => net.drive(name, value);

for (let frame = 0; frame < 40; frame++) {
  net.Iext.fill(0);
  const phase = frame / 39;
  drive('EXP', 0.85 + 1.05 * Math.max(0, 1 - Math.abs(phase - 0.25) * 4));
  drive('PROX', 0.85 + 1.05 * phase);
  drive('MOT', 0.85 + 1.05 * (0.5 + 0.5 * Math.sin(frame * 0.31)));
  drive('TCH', 0.85 + 1.05 * (frame >= 24 ? 0.72 : 0));
  drive('VIB', 0.85 + 1.05 * (frame === 12 ? 0.9 : 0));
  drive('LGT', 1.35);
  drive('SLF', 1.02);
  drive('REST', 1.75 + 0.2 * Math.cos(frame * 0.17));
  drive('MBON_A', 0.55); drive('MBON_V', 0.55);
  drive('VALP', 1.62 + (frame >= 24 ? 0.32 : 0));
  drive('VALN', 1.62 + (frame === 12 ? 0.45 : 0));
  drive('CPG_a', 1.50); drive('CPG_b', 1.47);
  drive('C_APPR', frame < 20 ? 0.42 : 0.12);
  drive('C_PERK', frame >= 10 && frame < 24 ? 0.58 : 0.10);
  drive('C_PLAY', frame >= 24 ? 0.64 : 0.08);
  drive('C_GROOM', 0.14); drive('C_MELT', 0.06); drive('C_FLEE', 0.0);
  let done = 0;
  while (done < 50) {
    const batch = Math.min(8, 50 - done);
    for (let i = 0; i < batch; i++) net.step();
    net.updateRates(batch);
    done += batch;
  }
}

const fixture = {
  schema: 1,
  upstream_commit: '3c6e27e3b55e4aff1d6c2c254713cdbd79099715',
  seed: 1234,
  frames: 40,
  frame_ms: 50,
  rates: Array.from(net.rate),
  voltage: Array.from(net.V),
  adaptation: Array.from(net.adapt),
  std_x: Array.from(net.stdX),
};
fs.mkdirSync(path.dirname(output), { recursive: true });
fs.writeFileSync(output, `${JSON.stringify(fixture)}\n`);
process.stdout.write(`${output}\n`);
