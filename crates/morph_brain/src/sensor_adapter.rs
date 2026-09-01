//! Named Morph command/population bridge for the nervous-system contract.

use std::collections::HashSet;

use lifecore::{
    MORPH_COMMAND_NAMES as CONTRACT_COMMAND_NAMES, MorphNervousSystemFrame, MorphPopulationFrame,
};

use crate::{COMMAND_NAMES, MORPH_COMMAND_COUNT, MorphDiagnostics, MorphOutput};

pub const REQUIRED_SEMANTIC_GROUPS: [&str; 10] = [
    "approach",
    "avoid",
    "turn_left",
    "turn_right",
    "explore",
    "interact",
    "play",
    "rest",
    "protect",
    "vocalize",
];

const SEMANTIC_COMMANDS: [(&str, &[&str]); 10] = [
    ("approach", &["C_APPR"]),
    ("avoid", &["C_FLEE"]),
    ("turn_left", &["C_TURN_L"]),
    ("turn_right", &["C_TURN_R"]),
    ("explore", &["C_PERK", "C_SAMPLE", "C_SNIFF"]),
    ("interact", &["C_TOUCH", "C_GRASP", "C_PULL"]),
    ("play", &["C_PLAY"]),
    ("rest", &["C_MELT", "C_GROOM"]),
    ("protect", &["C_FLEE", "C_PUSH", "C_PULL"]),
    // Upstream has no guessed numeric vocal command. A named orient/listen/perk
    // coalition is the explicit neural gate for LifeCore's vocal action.
    ("vocalize", &["C_LISTEN", "C_PERK", "C_APPR"]),
];

pub fn validate_nervous_system_command_map() -> Result<(), String> {
    let actual: HashSet<&str> = COMMAND_NAMES.into_iter().collect();
    if actual.len() != MORPH_COMMAND_COUNT {
        return Err("duplicate Morph command wire names".to_owned());
    }
    if COMMAND_NAMES != CONTRACT_COMMAND_NAMES {
        return Err("lifecore Morph command contract is stale".to_owned());
    }
    let groups: HashSet<&str> = SEMANTIC_COMMANDS.iter().map(|(group, _)| *group).collect();
    for required in REQUIRED_SEMANTIC_GROUPS {
        if !groups.contains(required) {
            return Err(format!("missing semantic command group {required}"));
        }
    }
    for (group, names) in SEMANTIC_COMMANDS {
        if names.is_empty() || names.iter().any(|name| !actual.contains(name)) {
            return Err(format!(
                "semantic group {group} references a missing command"
            ));
        }
    }
    Ok(())
}

#[must_use]
pub fn nervous_system_frame(
    output: MorphOutput,
    diagnostics: MorphDiagnostics,
) -> MorphNervousSystemFrame {
    let p = diagnostics.population_rates;
    MorphNervousSystemFrame {
        populations: MorphPopulationFrame {
            exp: p.exp,
            prox: p.prox,
            mot: p.mot,
            tch: p.tch,
            vib: p.vib,
            loom: p.loom,
            hab: p.hab,
            nov: p.nov,
            kc: p.kc,
            valp: p.valp,
            valn: p.valn,
            mbon_a: p.mbon_a,
            mbon_v: p.mbon_v,
            att: p.att,
            rest: p.rest,
        },
        winner_rate: output.winner_rate,
        confidence: output.confidence,
        valence: output.valence,
        arousal: output.arousal,
        conflict: output.conflict,
        turn: output.turn,
        command_rates: output.command_rates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_map_uses_every_real_name_without_numeric_guesses() {
        validate_nervous_system_command_map().expect("valid semantic map");
    }
}
