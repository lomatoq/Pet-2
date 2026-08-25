// Exports the colleague's canonical Morph neural topology into a stable asset
// consumed by the same-process Rust port. This is a development/provenance
// tool; Pet 2 does not need Node at runtime.

import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
const arg = name => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : null;
};
const root = path.resolve(arg('--root') || '_external/morph');
const output = path.resolve(arg('--output') || 'crates/morph_brain/assets/network-seed-1234.json');
const seed = Math.max(1, Number(arg('--seed') || 1234) >>> 0);
const commit = arg('--commit') || 'unknown';

const specPath = path.join(root, 'web', 'src', 'brain', 'spec.js');
if (!fs.existsSync(specPath)) throw new Error(`Morph spec not found: ${specPath}`);
const { buildBrain } = await import(pathToFileURL(specPath).href);
const net = buildBrain(seed);
const plain = value => Array.from(value || []);

const asset = {
  schema: 1,
  upstream_commit: commit,
  source_seed: seed,
  n: net.n,
  pops: net.pops.map(pop => ({ name: pop.name, start: pop.start, size: pop.size })),
  tau_m: plain(net.tauM),
  sigma: plain(net.sigma),
  t_ref: plain(net.tRef),
  pop_of: plain(net.popOf),
  thr_off: plain(net.thrOff),
  sfa_b: plain(net.sfaB),
  sfa_decay: plain(net.sfaDecay),
  row_start: plain(net.rowStart),
  edge_post: plain(net.ePost),
  edge_weight: plain(net.eW),
  edge_delay: plain(net.eDelay),
  edge_channel: plain(net.eChan),
  edge_std: plain(net.eStd),
  std_u: plain(net.stdU),
  std_recovery: plain(net.stdRec),
  traits: net.traits,
  meta: {
    plastic: {
      edge: plain(net.meta.plastic.edge),
      pre: plain(net.meta.plastic.pre),
      sign: plain(net.meta.plastic.sign),
    },
    kc_start: net.meta.kcStart,
    kc_size: net.meta.kcSize,
    bond_edges: plain(net.meta.bondEdges),
    dopamine_edges: plain(net.meta.daEdges),
    operant: {
      edge: plain(net.meta.operant.edge),
      pre: plain(net.meta.operant.pre),
      command: plain(net.meta.operant.cmd),
      names: plain(net.meta.operant.names),
    },
  },
};

fs.mkdirSync(path.dirname(output), { recursive: true });
fs.writeFileSync(output, `${JSON.stringify(asset)}\n`);
process.stdout.write(`${output}\nneurons=${net.n} synapses=${net.nEdges} populations=${net.pops.length}\n`);
