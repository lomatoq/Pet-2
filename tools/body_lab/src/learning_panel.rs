//! Live learning inspector and explicit teaching, using bounded local messages.
use egui::{self, Ui};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Clone)]
struct Cache {
    at: Instant,
    grounded: Value,
    vision: Value,
    training: Value,
    target: [f32; 2],
    program: String,
    consent: bool,
    message: String,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn read(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .filter(|v| v.len() < 512_000)
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or(Value::Null)
}
fn send(root: &Path, name: &str, v: &Value) -> Result<(), String> {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut f, v).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())?;
    f.persist(root.join(name))
        .map_err(|e| e.error.to_string())?;
    Ok(())
}
fn number(v: &Value, p: &str) -> u64 {
    v.pointer(p).and_then(Value::as_u64).unwrap_or(0)
}
fn text(v: &Value, p: &str) -> String {
    v.pointer(p).map_or("—".into(), |v| {
        v.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string())
    })
}
pub fn draw(ui: &mut Ui, root: &Path) {
    let id = ui.make_persistent_id(("v69-learning", root.to_string_lossy().to_string()));
    let mut c=ui.ctx().data_mut(|d|d.get_temp::<Cache>(id)).unwrap_or(Cache{at:Instant::now()-std::time::Duration::from_secs(2),grounded:Value::Null,vision:Value::Null,training:Value::Null,target:[0.5,0.95],program:r#"{"steps":[{"primitive":"inspect"},{"primitive":"approach"},{"primitive":"grip"},{"primitive":"carry","target":[0.5,0.95]},{"primitive":"release"},{"primitive":"wait_still","target":[0.5,0.95]}]}"#.into(),consent:false,message:String::new()});
    if c.at.elapsed().as_millis() > 750 {
        c.grounded = read(&root.join("grounded-status.json"));
        c.vision = read(&root.join("vision-status.json"));
        c.training = read(&root.join("vision-training-status.json"));
        c.at = Instant::now();
    }
    egui::CollapsingHeader::new("Living intelligence · goals, evidence and practice").default_open(true).show(ui,|ui|{
  ui.small(text(&c.grounded,"/message"));
  ui.horizontal_wrapped(|ui|{for (label,key) in [("Proposed","proposals"),("Executed","executed"),("Blocked","blocked"),("Measured outcomes","observed_outcomes"),("Weight updates","learning_updates"),("Resumed intentions","continuations")] {ui.label(format!("{label}: {}",number(&c.grounded,&format!("/stats/{key}"))));}});
  if let Some(x)=c.grounded.get("exercise").filter(|v|v.is_object()){
   let n=x["steps"].as_array().map_or(1,Vec::len);let index=x["index"].as_u64().unwrap_or(0) as usize;
   ui.label(format!("Intent: {} · difficulty {} · step {}/{}",text(x,"/family"),number(x,"/level"),index+1,n));
   ui.add(egui::ProgressBar::new(index as f32/n.max(1) as f32).text(text(x,&format!("/steps/{index}/primitive"))));
  }else{ui.label("No active practice task. Sleep, focus and care have priority.");}
  let instance=number(&c.grounded,"/instance");let fresh=now().abs_diff(number(&c.grounded,"/updated_unix_ms"))<5_000;
  ui.horizontal_wrapped(|ui|{for (label,family) in [("Roll","roll"),("Stop","stop"),("Shuttle","shuttle"),("Fetch","fetch"),("Explore around toy","orbit_inspect"),("Rebound","rebound")] {
    if ui.add_enabled(instance>0&&fresh,egui::Button::new(label)).clicked(){
     c.message=send(root,"grounded-control.json",&json!({"id":now(),"instance":instance,"issued_unix_ms":now(),"action":"practice","family":family,"level":0})).map_or_else(|e|e,|_|"Practice requested; waiting for safe admission".into());
    }
  }
   if ui.add_enabled(instance>0&&fresh,egui::Button::new("Stop practice")).clicked(){let _=send(root,"grounded-control.json",&json!({"id":now(),"instance":instance,"issued_unix_ms":now(),"action":"cancel"}));}
  });
  egui::CollapsingHeader::new("Give a goal, not a fixed animation").show(ui,|ui|{
    ui.horizontal(|ui|{ui.label("Target x/y:");ui.add(egui::DragValue::new(&mut c.target[0]).speed(0.01).range(0.04..=0.96));ui.add(egui::DragValue::new(&mut c.target[1]).speed(0.01).range(0.04..=0.96));});
    ui.horizontal_wrapped(|ui|{for (label,kind) in [("Place toy here","place_toy"),("Carry here","carry_toy"),("Stop the toy","stop_toy"),("Inspect","inspect_toy")] {
      if ui.add_enabled(instance>0&&fresh,egui::Button::new(label)).clicked(){
        c.message=send(root,"grounded-control.json",&json!({"id":now(),"instance":instance,"issued_unix_ms":now(),"action":"goal","goal":{"kind":kind,"target":c.target,"tolerance":0.05}})).map_or_else(|e|e,|_|"Goal sent to the bounded planner".into());
      }
    }});
    ui.label(format!("Learned reusable compositions: {}",c.grounded["learned_programs"].as_array().map_or(0,Vec::len)));
  });
  egui::CollapsingHeader::new("Compose an exercise · safe task language").show(ui,|ui|{
    ui.add(egui::TextEdit::multiline(&mut c.program).font(egui::TextStyle::Monospace).desired_rows(4).char_limit(20000));
    if ui.add_enabled(instance>0&&fresh,egui::Button::new("Validate and queue this composition")).clicked(){
      match serde_json::from_str::<Value>(&c.program){Ok(program)=>{c.message=send(root,"grounded-control.json",&json!({"id":now(),"instance":instance,"issued_unix_ms":now(),"action":"program","program":program})).map_or_else(|e|e,|_|"Composition submitted; Pet validates every step".into());},Err(e)=>c.message=e.to_string()}
    }
    ui.small("Up to 24 bounded primitives. No code, OS actions, forged progress or rewards.");
  });
  if let Some(skills)=c.grounded["skills"].as_array(){for s in skills {let level=number(s,"/level") as usize;let t=&s["trials"][level];ui.small(format!("{} · level {} · confirmed {}/{}",text(s,"/family"),level+1,number(t,"/successes"),number(t,"/attempts")));}}
  egui::CollapsingHeader::new("What changed after my action?").show(ui,|ui|{
   if let Some(rows)=c.grounded["recent_experience"].as_array(){for x in rows {ui.small(format!("#{} {:?} · object {} · {} · error {}",number(x,"/id"),x["action"],number(x,"/object_id"),text(x,"/outcome"),text(x,"/prediction_error")));}}
  });
 });
    egui::CollapsingHeader::new("Local vision · explicit correction and trainable weights").default_open(true).show(ui,|ui|{
  ui.label(format!("Model: {} · {} · paused {}",text(&c.vision,"/state"),text(&c.vision,"/scene"),text(&c.vision,"/paused")));
  ui.small(format!("Adapter: {}",text(&c.vision,"/adapter")));
  ui.small(format!("Observed {} · accepted {} · ambiguous {} · latency {} ms",number(&c.vision,"/completed"),number(&c.vision,"/accepted"),number(&c.vision,"/ambiguous"),text(&c.vision,"/last_latency_ms")));
  ui.checkbox(&mut c.consent,"Save this confirmed example locally for learning (private visual features)");
  let observation=number(&c.vision,"/teaching/sample_id");let instance=number(&c.vision,"/teaching/instance");let age=number(&c.vision,"/teaching/sample_age_seconds");
  ui.small(format!("Correct the LAST observation #{observation}, {age}s old, not the Console currently on screen. No screenshot history is stored automatically."));
  ui.horizontal_wrapped(|ui|{for (label,key) in [("Code","A"),("Document","B"),("Photo / video","C"),("Artwork","D"),("Game","E"),("Chart","F"),("Desktop","G"),("Web page","H"),("Unknown","I")] {
    if ui.add_enabled(c.consent&&instance>0&&observation>0&&age<=30,egui::Button::new(label)).clicked(){
     c.message=send(root,"vision-teaching-request.json",&json!({"id":now(),"instance":instance,"issued_unix_ms":now(),"action":"teach","observation_id":observation,"label":key,"consent_features":true})).map_or_else(|e|e,|_|"Correction submitted; check acknowledgement".into());c.consent=false;
    }
  }});
  ui.small(text(&c.vision,"/teaching/message"));
  ui.horizontal_wrapped(|ui|{for (label,action) in [("Disable adapter","disable_adapter"),("Enable validated adapter","enable_adapter"),("Delete examples and training copies","delete_examples"),("Promote validated candidate","promote_adapter")] {
    if ui.add_enabled(instance>0,egui::Button::new(label)).clicked(){let _=send(root,"vision-teaching-request.json",&json!({"id":now(),"instance":instance,"issued_unix_ms":now(),"action":action,"consent_features":false}));}
  }});
  ui.small("Training is staged: confirmed examples → separate validation → native check → explicit promotion. The base model remains recoverable.");
  ui.small(format!("Trainer: {} · {}",text(&c.training,"/state"),text(&c.training,"/message")));
  if ui.button("Train from my confirmed examples").clicked(){
    c.message=start_training(root).map_or_else(|e|e,|_|"Local trainer started; active weights remain unchanged until validation and promotion".into());
  }
  ui.small("Training uses the configured local Python/CUDA worker. Normal Pet inference remains native and does not require Python.");
  ui.label(&c.message);
 });
    ui.ctx().data_mut(|d| d.insert_temp(id, c));
}

fn start_training(root: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let package = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("package path")?
        .to_path_buf();
    let cfg = read(&package.join("config/learning-runtime.json"));
    let python = cfg["training_python"]
        .as_str()
        .ok_or("Local trainer is not configured")?;
    let model = cfg["training_model"]
        .as_str()
        .ok_or("Training checkpoint is not configured")?;
    let log =
        fs::File::create(root.join("vision-training-launch.log")).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(python);
    cmd.arg(package.join("scripts/train_personal_adapter.py"))
        .arg("--profile")
        .arg(root)
        .arg("--model")
        .arg(model)
        .arg("--package")
        .arg(&package);
    cmd.stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}
