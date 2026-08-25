// Pet 2 <-> Thandorcat/morph development bridge.
//
// The protocol is deliberately JSONL and numeric. Morph remains a replaceable
// suggestion provider: it never receives pixels/text and never writes Pet body
// particles, audio samples, windows, or native input.

import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';
import { pathToFileURL } from 'node:url';
import { performance } from 'node:perf_hooks';

const args = process.argv.slice(2);
const arg = name => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : null;
};
const root = arg('--root');
const statePath = arg('--state');
const seed = Math.max(1, Number(arg('--seed') || 1234) >>> 0);

if (!root) throw new Error('--root is required');
const simPath = path.join(path.resolve(root), 'web', 'src', 'sim.js');
if (!fs.existsSync(simPath)) throw new Error(`Morph sim.js not found: ${simPath}`);

const { Sim } = await import(pathToFileURL(simPath).href);
const sim = new Sim({ seed, stage: { w: 900, h: 560 }, terrainProfile: 'flat' });
if (statePath && fs.existsSync(statePath)) {
  try { sim.load(JSON.parse(fs.readFileSync(statePath, 'utf8'))); }
  catch (error) { process.stderr.write(`Morph state ignored: ${error.message}\n`); }
}

const COMMANDS = ['C_FLEE', 'C_APPR', 'C_PERK', 'C_MELT', 'C_GROOM', 'C_PLAY'];
let saveElapsedMs = 0;
let responseCount = 0;

function finite(value, fallback = 0) {
  const n = Number(value);
  return Number.isFinite(n) ? n : fallback;
}

function clamp(value, low = 0, high = 1) {
  return Math.min(high, Math.max(low, finite(value, low)));
}

function syncObservation(observation) {
  const body = observation.body || {};
  const bx = clamp(body.x, 0, 1) * sim.stage.w;
  const by = clamp(body.y, 0, 1) * sim.stage.h;
  sim.body.stopMotion();
  sim.body.moveTo(bx, by);
  sim.body.vel.x = clamp(body.vx, -4, 4) * sim.stage.w;
  sim.body.vel.y = clamp(body.vy, -4, 4) * sim.stage.h;

  const cursor = observation.cursor || {};
  if (cursor.present !== false) {
    sim.world.pointerMove(
      clamp(cursor.x, 0, 1) * sim.stage.w,
      clamp(cursor.y, 0, 1) * sim.stage.h,
    );
  } else {
    sim.world.pointerLeave();
  }
  if (observation.pointer?.pressed) sim.world.poke(0.72);
  const collision = clamp(body.collision, 0, 1);
  if (collision > 0.01) sim.world.poke(0.25 + collision * 0.75);
  sim.world.hour = clamp(observation.time_of_day_01, 0, 1) * 24;
}

function persist() {
  if (!statePath) return;
  const target = path.resolve(statePath);
  const temporary = `${target}.tmp-${process.pid}`;
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(temporary, JSON.stringify(sim.toJSON()));
  fs.renameSync(temporary, target);
}

function behavior(observation, processingUs) {
  const rates = Object.fromEntries(COMMANDS.map(name => [name, finite(sim.rates[name])]));
  const winnerRate = finite(sim.readout.winnerRate);
  const conflict = clamp(sim.readout.conflict);
  return {
    schema: 1,
    sequence: Math.max(0, Number(observation.sequence) || 0),
    winner: String(sim.readout.winner || 'idle'),
    winner_rate: winnerRate,
    confidence: clamp(winnerRate / 45) * (1 - conflict),
    attention: String(sim.readout.attTargetName || 'wander'),
    valence: clamp(sim.readout.valence, -1, 1),
    arousal: clamp(sim.readout.arousal),
    conflict,
    turn: clamp((finite(sim.rates.C_TURN_R) - finite(sim.rates.C_TURN_L)) / 45, -1, 1),
    rates,
    needs: {
      energy: clamp(sim.mods.energy), social: clamp(sim.mods.social),
      hunger: clamp(sim.mods.hunger), fun: clamp(sim.mods.funNeed),
      curiosity: clamp(sim.mods.curiosityDrive), threat: clamp(sim.mods.threat),
      comfort: clamp(sim.mods.comfort), bond: clamp(sim.mods.bond),
    },
    processing_us: processingUs,
    state_bytes: responseCount % 20 === 0 ? JSON.stringify(sim.toJSON()).length : null,
  };
}

const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of input) {
  if (!line.trim()) continue;
  const started = performance.now();
  try {
    const observation = JSON.parse(line);
    if (observation.schema !== 1) throw new Error(`unsupported observation schema ${observation.schema}`);
    syncObservation(observation);
    const dtMs = clamp(observation.dt_ms, 1, 100);
    sim.advanceRealtime(dtMs, 1, 16.67);
    saveElapsedMs += dtMs;
    responseCount++;
    if (saveElapsedMs >= 5000) { saveElapsedMs %= 5000; persist(); }
    const processingUs = Math.max(0, (performance.now() - started) * 1000);
    process.stdout.write(`${JSON.stringify(behavior(observation, processingUs))}\n`);
  } catch (error) {
    process.stdout.write(`${JSON.stringify({ schema: 1, error: String(error?.message || error) })}\n`);
  }
}

try { persist(); } catch (error) { process.stderr.write(`Morph state save failed: ${error.message}\n`); }
