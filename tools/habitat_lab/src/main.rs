use std::{env, error::Error, fmt::Write as _, fs, path::PathBuf};

use glam::Vec2;
use lifecore::stable_hash_bytes;
use pet_ecology::{EcologyState, ObjectLifecycle, ObjectPhysicsConfig, step_object};

#[derive(Debug)]
struct Arguments {
    seed: u64,
    ticks: u64,
    speed: f32,
    scenario: String,
    brain_mode: String,
    focus_mode: bool,
    load: Option<PathBuf>,
    save: Option<PathBuf>,
    trace: Option<PathBuf>,
    screenshot: Option<PathBuf>,
}

impl Default for Arguments {
    fn default() -> Self {
        Self {
            seed: 42,
            ticks: 240,
            speed: 1.0,
            scenario: "ecology_smoke".into(),
            brain_mode: "morphic".into(),
            focus_mode: false,
            load: None,
            save: None,
            trace: None,
            screenshot: None,
        }
    }
}

impl Arguments {
    fn parse() -> Result<Self, Box<dyn Error>> {
        let mut parsed = Self::default();
        let mut arguments = env::args().skip(1).peekable();
        while let Some(argument) = arguments.next() {
            let value = |name: &str, arguments: &mut std::iter::Peekable<_>| {
                arguments
                    .next()
                    .ok_or_else(|| format!("{name} requires a value"))
            };
            match argument.as_str() {
                "--seed" => parsed.seed = value("--seed", &mut arguments)?.parse()?,
                "--ticks" | "--step" => {
                    parsed.ticks = value(argument.as_str(), &mut arguments)?.parse()?;
                }
                "--pause" => parsed.ticks = 0,
                "--speed" => parsed.speed = value("--speed", &mut arguments)?.parse()?,
                "--scenario" => parsed.scenario = value("--scenario", &mut arguments)?,
                "--brain-mode" => parsed.brain_mode = value("--brain-mode", &mut arguments)?,
                "--focus-mode" => parsed.focus_mode = true,
                "--load" => parsed.load = Some(value("--load", &mut arguments)?.into()),
                "--save" => parsed.save = Some(value("--save", &mut arguments)?.into()),
                "--trace" => parsed.trace = Some(value("--trace", &mut arguments)?.into()),
                "--screenshot" => {
                    parsed.screenshot = Some(value("--screenshot", &mut arguments)?.into());
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }
        if ![
            "classic",
            "morphic",
            "fusion",
            "morph-shadow",
            "morph-fusion",
        ]
        .contains(&parsed.brain_mode.as_str())
        {
            return Err(format!("unsupported brain mode: {}", parsed.brain_mode).into());
        }
        if !parsed.speed.is_finite() || !(0.05..=64.0).contains(&parsed.speed) {
            return Err("--speed must be finite and between 0.05 and 64".into());
        }
        Ok(parsed)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::parse()?;
    let mut state = if let Some(path) = &arguments.load {
        EcologyState::restore(serde_json::from_slice(&fs::read(path)?)?)?
    } else {
        EcologyState::new(arguments.seed)
    };
    seed_scenario(&mut state, &arguments.scenario)?;
    let dt = (1.0 / 120.0 * arguments.speed).min(1.0 / 30.0);
    for _ in 0..arguments.ticks {
        for object in &mut state.objects {
            step_object(object, ObjectPhysicsConfig::default(), dt);
        }
        state.metabolism.advance(dt);
    }
    state.validate()?;
    let encoded = serde_json::to_vec_pretty(&state)?;
    let summary = serde_json::json!({
        "scenario": arguments.scenario,
        "seed": arguments.seed,
        "ticks": arguments.ticks,
        "speed": arguments.speed,
        "brain_mode": arguments.brain_mode,
        "focus_mode": arguments.focus_mode,
        "ecology_schema": state.schema_version,
        "active_goal": null,
        "phase": null,
        "reason_code": "no_eligible_episode",
        "contacts": [],
        "outcomes": [],
        "skill_error": null,
        "object_positions": state.objects.iter().map(|object| object.position.to_array()).collect::<Vec<_>>(),
        "object_velocities": state.objects.iter().map(|object| object.velocity.to_array()).collect::<Vec<_>>(),
        "state_hash": stable_hash_bytes(&encoded),
        "finite_and_bounded": true,
    });
    let summary_json = serde_json::to_string_pretty(&summary)?;
    println!("{summary_json}");
    if let Some(path) = &arguments.save {
        fs::write(path, &encoded)?;
    }
    if let Some(path) = &arguments.trace {
        fs::write(path, format!("{summary_json}\n"))?;
    }
    if let Some(path) = &arguments.screenshot {
        fs::write(path, habitat_svg(&state, &arguments))?;
    }
    Ok(())
}

fn seed_scenario(state: &mut EcologyState, scenario: &str) -> Result<(), Box<dyn Error>> {
    let orb = &mut state.objects[0];
    match scenario {
        "ecology_smoke" => {
            orb.position = Vec2::new(0.42, 0.47);
            orb.velocity = Vec2::new(0.58, -0.31);
            orb.lifecycle = ObjectLifecycle::Free;
        }
        "orb_drag_throw" => {
            orb.position = Vec2::new(0.28, 0.52);
            orb.velocity = Vec2::new(1.75, -0.42);
            orb.lifecycle = ObjectLifecycle::Free;
        }
        "focus_mode_return_home" => {
            orb.position = state.den.anchor;
            orb.velocity = Vec2::ZERO;
            orb.lifecycle = ObjectLifecycle::Sleeping;
        }
        other => return Err(format!("unknown deterministic scenario: {other}").into()),
    }
    Ok(())
}

fn habitat_svg(state: &EcologyState, arguments: &Arguments) -> String {
    const WIDTH: f32 = 1_280.0;
    const HEIGHT: f32 = 720.0;
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720" viewBox="0 0 1280 720">
<rect width="1280" height="720" fill="#10131b"/>
<rect x="210" y="115" width="410" height="255" rx="12" fill="#252a38" stroke="#75809c" stroke-width="3"/>
<rect x="735" y="235" width="330" height="225" rx="12" fill="#202532" stroke="#65708b" stroke-width="3"/>
"##,
    );
    for row in 0..9 {
        for column in 0..16 {
            let energy = ((column * 13 + row * 7 + arguments.seed as usize) % 17) as f32 / 16.0;
            let opacity = 0.025 + energy * 0.075;
            let _ = writeln!(
                svg,
                r##"<rect x="{}" y="{}" width="80" height="80" fill="#7bdfff" opacity="{opacity:.3}"/>"##,
                column * 80,
                row * 80
            );
        }
    }
    let den = state.den.anchor * Vec2::new(WIDTH, HEIGHT);
    let _ = writeln!(
        svg,
        r##"<path d="M {x:.1} {y:.1} q -72 -55 -92 8 q 28 72 96 38" fill="#342b55" stroke="#b39cff" stroke-width="5" opacity="0.92"/>"##,
        x = den.x,
        y = den.y
    );
    for object in &state.objects {
        let point = object.position * Vec2::new(WIDTH, HEIGHT);
        let radius = object.radius_px_at_reference * HEIGHT / 1_152.0;
        let _ = writeln!(
            svg,
            r##"<circle cx="{:.1}" cy="{:.1}" r="{radius:.1}" fill="#65ddff" stroke="#eaffff" stroke-width="4"/><circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="#ffffff" opacity="0.65"/>"##,
            point.x,
            point.y,
            point.x - radius * 0.28,
            point.y - radius * 0.31,
            radius * 0.19,
        );
    }
    svg.push_str(
        r##"<ellipse cx="640" cy="360" rx="58" ry="72" fill="#c996ff" stroke="#f2e6ff" stroke-width="5"/><circle cx="620" cy="350" r="9" fill="#111522"/><circle cx="660" cy="350" r="9" fill="#111522"/>"##,
    );
    let _ = writeln!(
        svg,
        r##"<text x="28" y="42" fill="#f4f7ff" font-family="Segoe UI, sans-serif" font-size="24">Habitat Lab · {} · {} · {} ticks</text>"##,
        arguments.scenario, arguments.brain_mode, arguments.ticks
    );
    svg.push_str("</svg>\n");
    svg
}

fn print_help() {
    println!(
        "Habitat Lab\n\
         --scenario ecology_smoke|orb_drag_throw|focus_mode_return_home\n\
         --seed N --ticks N|--step N|--pause --speed X\n\
         --brain-mode classic|morphic|fusion|morph-shadow|morph-fusion --focus-mode\n\
         --load ecology.json --save ecology.json --trace trace.json --screenshot habitat.svg"
    );
}
