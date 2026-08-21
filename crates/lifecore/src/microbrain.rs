use std::array;

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::{ACTION_COUNT, BrainGenome, EXPRESSION_READOUT_COUNT};

pub const NEURON_COUNT: usize = 64;
pub const SENSOR_INPUT_COUNT: usize = 32;
const MAX_PLASTIC_SYNAPSES: usize = 384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Population {
    Orientation,
    Approach,
    Avoidance,
    Play,
    Explore,
    Rest,
    Vocal,
    Social,
    Inhibition,
    Persistence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputConnection {
    pub input: u8,
    pub post: u8,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecurrentConnection {
    pub pre: u8,
    pub post: u8,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlasticSynapse {
    pub pre: usize,
    pub post: usize,
    pub baseline_weight: f32,
    pub delta_weight: f32,
    pub eligibility: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicroBrain {
    pub state: Vec<f32>,
    pub biases: Vec<f32>,
    pub time_constants: Vec<f32>,
    pub populations: Vec<Population>,
    pub input_connections: Vec<InputConnection>,
    pub recurrent_connections: Vec<RecurrentConnection>,
    pub plastic_synapses: Vec<PlasticSynapse>,
    pub stable_state: Vec<f32>,
    pub ticks_since_normalization: u32,
    pub divergence_count: u32,
    pub genome: BrainGenome,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrainReadouts {
    pub actions: [f32; ACTION_COUNT],
    pub expressions: [f32; EXPRESSION_READOUT_COUNT],
}

impl MicroBrain {
    #[must_use]
    pub fn new(genome: BrainGenome) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(genome.network_seed);
        let populations: Vec<_> = (0..NEURON_COUNT).map(population_for).collect();
        let biases = (0..NEURON_COUNT)
            .map(|index| archetype_bias(populations[index]) + signed(&mut rng) * 0.08)
            .collect();
        let time_constants = (0..NEURON_COUNT)
            .map(|_| range(&mut rng, 0.1, 4.0) * genome.time_constant_scale)
            .collect();

        let mut input_connections = Vec::with_capacity(160);
        add_archetype_inputs(&mut input_connections);
        for post in 0..NEURON_COUNT {
            for _ in 0..2 {
                input_connections.push(InputConnection {
                    input: (rng.next_u32() as usize % SENSOR_INPUT_COUNT) as u8,
                    post: post as u8,
                    weight: signed(&mut rng) * 0.32,
                });
            }
        }

        let desired_connections =
            (NEURON_COUNT * NEURON_COUNT) as f32 * genome.recurrent_connectivity;
        let mut recurrent_connections = Vec::with_capacity(desired_connections as usize);
        let mut plastic_synapses = Vec::new();
        while recurrent_connections.len() < desired_connections as usize {
            let pre = rng.next_u32() as usize % NEURON_COUNT;
            let post = rng.next_u32() as usize % NEURON_COUNT;
            if pre == post
                || recurrent_connections
                    .iter()
                    .any(|connection: &RecurrentConnection| {
                        usize::from(connection.pre) == pre && usize::from(connection.post) == post
                    })
            {
                continue;
            }
            let incoming_scale = 1.0 / (desired_connections / NEURON_COUNT as f32).sqrt();
            let weight = signed(&mut rng) * 0.72 * incoming_scale;
            recurrent_connections.push(RecurrentConnection {
                pre: pre as u8,
                post: post as u8,
                weight,
            });
            let safety_locked =
                matches!(populations[post], Population::Avoidance | Population::Rest);
            if !safety_locked
                && plastic_synapses.len() < MAX_PLASTIC_SYNAPSES
                && unit(&mut rng) < genome.plastic_fraction
            {
                plastic_synapses.push(PlasticSynapse {
                    pre,
                    post,
                    baseline_weight: weight,
                    delta_weight: 0.0,
                    eligibility: 0.0,
                });
            }
        }

        Self {
            state: vec![0.0; NEURON_COUNT],
            biases,
            time_constants,
            populations,
            input_connections,
            recurrent_connections,
            plastic_synapses,
            stable_state: vec![0.0; NEURON_COUNT],
            ticks_since_normalization: 0,
            divergence_count: 0,
            genome,
        }
    }

    pub fn tick(&mut self, inputs: &[f32; SENSOR_INPUT_COUNT], dt: f32) -> BrainReadouts {
        let dt = dt.clamp(0.0, 0.25);
        let mut totals: [f32; NEURON_COUNT] = array::from_fn(|index| self.biases[index]);
        for connection in &self.input_connections {
            totals[usize::from(connection.post)] +=
                connection.weight * inputs[usize::from(connection.input)].clamp(-1.0, 1.0);
        }
        for connection in &self.recurrent_connections {
            totals[usize::from(connection.post)] +=
                connection.weight * self.state[usize::from(connection.pre)];
        }
        for synapse in &self.plastic_synapses {
            totals[synapse.post] += synapse.delta_weight * self.state[synapse.pre];
        }

        let previous: [f32; NEURON_COUNT] = array::from_fn(|index| self.state[index]);
        for ((state, total), time_constant) in self
            .state
            .iter_mut()
            .zip(totals)
            .zip(self.time_constants.iter().copied())
        {
            let target = total.clamp(-8.0, 8.0).tanh();
            let rate = (dt / time_constant.max(0.05)).clamp(0.0, 1.0);
            *state += rate * (-*state + target);
        }
        let eligibility_decay = (-dt / self.genome.eligibility_tau.max(0.05)).exp();
        for synapse in &mut self.plastic_synapses {
            synapse.eligibility = (synapse.eligibility * eligibility_decay
                + previous[synapse.pre] * self.state[synapse.post])
                .clamp(-4.0, 4.0);
        }

        if self
            .state
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 1.001)
        {
            self.state.copy_from_slice(&self.stable_state);
            for synapse in &mut self.plastic_synapses {
                synapse.delta_weight = 0.0;
                synapse.eligibility = 0.0;
            }
            self.divergence_count = self.divergence_count.saturating_add(1);
        } else {
            self.stable_state.copy_from_slice(&self.state);
        }

        self.ticks_since_normalization = self.ticks_since_normalization.saturating_add(1);
        if self.ticks_since_normalization >= 200 {
            self.normalize_plastic_incoming();
            self.ticks_since_normalization = 0;
        }
        self.readouts()
    }

    pub fn apply_reward(&mut self, reward: f32) {
        let reward = reward.clamp(-1.0, 1.0);
        for synapse in &mut self.plastic_synapses {
            synapse.delta_weight += self.genome.learning_rate * reward * synapse.eligibility;
            synapse.delta_weight *= self.genome.plastic_decay;
            synapse.delta_weight = synapse.delta_weight.clamp(
                -self.genome.maximum_plastic_delta,
                self.genome.maximum_plastic_delta,
            );
        }
    }

    pub fn consolidate(&mut self) {
        for synapse in &mut self.plastic_synapses {
            if synapse.eligibility.abs() < 0.02 {
                synapse.delta_weight *= 0.985;
            }
            synapse.eligibility *= 0.25;
        }
        self.normalize_plastic_incoming();
    }

    pub fn apply_development(&mut self, genome: &BrainGenome, mutation_rate: f32) {
        let ratio = (genome.time_constant_scale / self.genome.time_constant_scale.max(0.01))
            .clamp(0.85, 1.15);
        for time_constant in &mut self.time_constants {
            *time_constant = (*time_constant * ratio).clamp(0.1, 4.0);
        }
        let weight_scale = (1.0
            + (genome.recurrent_connectivity - self.genome.recurrent_connectivity)
                * mutation_rate.clamp(0.0, 0.2))
        .clamp(0.96, 1.04);
        for connection in &mut self.recurrent_connections {
            if !matches!(
                self.populations[usize::from(connection.post)],
                Population::Avoidance | Population::Rest
            ) {
                connection.weight = (connection.weight * weight_scale).clamp(-0.75, 0.75);
            }
        }
        self.genome = genome.clone();
        self.normalize_plastic_incoming();
    }

    #[must_use]
    pub fn plastic_weight_range(&self) -> (f32, f32) {
        self.plastic_synapses
            .iter()
            .map(|synapse| synapse.baseline_weight + synapse.delta_weight)
            .fold((0.0_f32, 0.0_f32), |(minimum, maximum), value| {
                (minimum.min(value), maximum.max(value))
            })
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.state.len() == NEURON_COUNT
            && self.biases.len() == NEURON_COUNT
            && self.time_constants.len() == NEURON_COUNT
            && self.populations.len() == NEURON_COUNT
            && self.stable_state.len() == NEURON_COUNT
            && self.plastic_synapses.len() <= MAX_PLASTIC_SYNAPSES
            && self
                .state
                .iter()
                .chain(&self.biases)
                .chain(&self.time_constants)
                .chain(&self.stable_state)
                .all(|value| value.is_finite())
            && self.plastic_synapses.iter().all(|synapse| {
                synapse.pre < NEURON_COUNT
                    && synapse.post < NEURON_COUNT
                    && synapse.delta_weight.abs()
                        <= self.genome.maximum_plastic_delta + f32::EPSILON
                    && synapse.eligibility.is_finite()
            })
    }

    fn readouts(&self) -> BrainReadouts {
        let actions = array::from_fn(|action| {
            let first = (action * 7 + 3) % NEURON_COUNT;
            let second = (action * 11 + 17) % NEURON_COUNT;
            let third = (action * 13 + 29) % NEURON_COUNT;
            (self.state[first] * 0.52 + self.state[second] * 0.31 + self.state[third] * 0.17)
                .clamp(-1.0, 1.0)
        });
        let expressions = array::from_fn(|readout| {
            let first = (readout * 5 + 1) % NEURON_COUNT;
            let second = (readout * 9 + 7) % NEURON_COUNT;
            (self.state[first] * 0.66 + self.state[second] * 0.34).clamp(-1.0, 1.0)
        });
        BrainReadouts {
            actions,
            expressions,
        }
    }

    fn normalize_plastic_incoming(&mut self) {
        let mut incoming = [0.0_f32; NEURON_COUNT];
        for synapse in &self.plastic_synapses {
            incoming[synapse.post] += synapse.delta_weight.abs();
        }
        for synapse in &mut self.plastic_synapses {
            let total = incoming[synapse.post];
            if total > 1.25 {
                synapse.delta_weight *= 1.25 / total;
            }
            synapse.delta_weight = synapse.delta_weight.clamp(
                -self.genome.maximum_plastic_delta,
                self.genome.maximum_plastic_delta,
            );
        }
    }
}

fn add_archetype_inputs(connections: &mut Vec<InputConnection>) {
    let mut add_population = |population: Population, input: usize, weight: f32| {
        for neuron in 0..NEURON_COUNT {
            if population_for(neuron) == population {
                connections.push(InputConnection {
                    input: input as u8,
                    post: neuron as u8,
                    weight,
                });
            }
        }
    };
    add_population(Population::Orientation, 0, 0.55);
    add_population(Population::Orientation, 1, 0.55);
    add_population(Population::Approach, 17, 0.58);
    add_population(Population::Approach, 31, 0.45);
    add_population(Population::Avoidance, 3, 0.72);
    add_population(Population::Avoidance, 21, 0.78);
    add_population(Population::Play, 2, 0.55);
    add_population(Population::Play, 18, 0.66);
    add_population(Population::Explore, 23, 0.72);
    add_population(Population::Rest, 16, 0.82);
    add_population(Population::Vocal, 17, 0.40);
    add_population(Population::Social, 30, 0.50);
    add_population(Population::Inhibition, 26, 0.66);
    add_population(Population::Inhibition, 29, 0.72);
    add_population(Population::Persistence, 27, 0.46);
}

fn population_for(index: usize) -> Population {
    match index {
        0..=7 => Population::Orientation,
        8..=13 => Population::Approach,
        14..=19 => Population::Avoidance,
        20..=25 => Population::Play,
        26..=31 => Population::Explore,
        32..=37 => Population::Rest,
        38..=43 => Population::Vocal,
        44..=49 => Population::Social,
        50..=56 => Population::Inhibition,
        _ => Population::Persistence,
    }
}

fn archetype_bias(population: Population) -> f32 {
    match population {
        Population::Orientation => 0.04,
        Population::Approach => 0.02,
        Population::Avoidance => -0.04,
        Population::Play => 0.01,
        Population::Explore => 0.03,
        Population::Rest => -0.03,
        Population::Vocal => -0.08,
        Population::Social => 0.01,
        Population::Inhibition => -0.02,
        Population::Persistence => 0.02,
    }
}

fn unit(rng: &mut impl RngCore) -> f32 {
    (f64::from(rng.next_u32()) / f64::from(u32::MAX)) as f32
}

fn signed(rng: &mut impl RngCore) -> f32 {
    unit(rng) * 2.0 - 1.0
}

fn range(rng: &mut impl RngCore, minimum: f32, maximum: f32) -> f32 {
    minimum + (maximum - minimum) * unit(rng)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_brain_is_sparse_bounded_and_deterministic() {
        let genome = crate::Genome::from_seed(12).brain;
        let first = MicroBrain::new(genome.clone());
        let second = MicroBrain::new(genome);
        assert_eq!(first, second);
        assert!((327..=492).contains(&first.recurrent_connections.len()));
        assert!(first.plastic_synapses.len() <= MAX_PLASTIC_SYNAPSES);
        assert!(first.is_valid());
    }

    #[test]
    fn reward_never_breaks_plastic_bounds() {
        let mut brain = MicroBrain::new(crate::Genome::from_seed(15).brain);
        for _ in 0..20_000 {
            brain.tick(&[0.75; SENSOR_INPUT_COUNT], 0.05);
            brain.apply_reward(1.0);
        }
        assert!(brain.is_valid());
    }
}
