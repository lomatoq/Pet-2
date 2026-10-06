use pet_vision::{Result,VisionEngine,ConnectorAdapter};
use std::{path::PathBuf,time::Instant};
fn main()->Result<()> {
    let args:Vec<_>=std::env::args().skip(1).collect();
    if args.len()<3{return Err("usage: vision_probe MODEL_DIR DLL [--adapter PATH] [--features-dir DIR] IMAGE...".into())}
    let mut engine=VisionEngine::load(&PathBuf::from(&args[0]),&PathBuf::from(&args[1]))?;
    let mut i=2;let mut features_dir=None;
    while i<args.len()&&args[i].starts_with("--") {
        let value=args.get(i+1).ok_or("missing option value")?;
        match args[i].as_str(){
            "--adapter"=>engine.set_adapter(Some(ConnectorAdapter::load(&PathBuf::from(value))?))?,
            "--features-dir"=>{let p=PathBuf::from(value);std::fs::create_dir_all(&p)?;features_dir=Some(p);},
            _=>return Err("unknown probe option".into()),
        }i+=2;
    }
    for path in &args[i..] {
        let img=image::open(path)?.into_rgb8();let start=Instant::now();
        let prediction=engine.classify(img.width(),img.height(),img.into_raw(),256)?;
        if let Some(dir)=&features_dir {
            let stem=PathBuf::from(path);let name=stem.file_stem().ok_or("image name")?.to_string_lossy();
            let out=dir.join(format!("{name}.json"));
            if out.exists(){return Err("Refusing to replace an existing feature example".into())}
            let example=engine.take_training_features().ok_or("missing features")?;
            if !example.valid(){return Err("invalid feature example".into())}
            serde_json::to_writer(std::fs::File::create(out)?,&example)?;
        }
        println!("{}",serde_json::json!({"image":path,"prediction":prediction,"accepted":prediction.trustworthy(),"total_ms":start.elapsed().as_secs_f32()*1000.0}));
    }Ok(())
}
