//! Explicit headless TV operations; never create a GUI or toggle power blindly.
use crate::{
    tv_control::{self, PowerAction},
    tvs,
    windows_audio::OperationLock,
    windows_tv::{self, WindowsTvControls},
};
pub fn run(args: &[String]) -> Result<i32, String> {
    let _operation = crate::diagnostics::Operation::start("tv.cli", None);
    if let Some(command) = args.first().filter(|c| {
        [
            "list", "pair", "check", "learn", "wake", "status", "off", "input",
        ]
        .contains(&c.as_str())
    }) {
        crate::diagnostics::event("tv.cli.intent", "selected", command);
    }
    match args{
  [command] if command=="list"=>{
   for tv in tvs::load(&tvs::path()?)?{println!("{} — {} · {} · {}",tv.name,tv.address,if tv.credential.is_some(){"Paired"}else{"Not paired"},if tv.network.targets().is_empty(){"Wake not configured"}else{"Wake available"});}
   Ok(0)
  }
  [command,name] if command=="report"=>{let _lock=OperationLock::acquire()?;let saved=tvs::load(&tvs::path()?)?;let tv=saved.iter().find(|t|t.name.eq_ignore_ascii_case(name)).ok_or("No TV with that name.")?;let path=crate::samsung::export(tv)?;println!("Redacted report: {}",path.display());Ok(0)}
  [command,name,id] if command=="input"=>{
   let _lock=OperationLock::acquire()?;let tvs=tvs::load(&tvs::path()?)?;
   let tv=tvs.iter().find(|t|t.name.eq_ignore_ascii_case(name)).ok_or("No saved TV has that name.")?;
   let report=tv_control::switch_input(&mut WindowsTvControls::new(tv),id)?;
   println!("{}\n{}",report.message,report.details);Ok(0)
  }
  [command,name] if ["pair","check","learn","wake","status","off"].contains(&command.as_str())=>{
   let _lock=OperationLock::acquire()?;let path=tvs::path()?;let expected=tvs::load(&path)?;
   let tv=expected.iter().find(|t|t.name.eq_ignore_ascii_case(name)).ok_or("No saved TV has that name. Add it in the TVs view first.")?;
   match command.as_str(){
    "wake"=>{let report=crate::tv_control::Backend::wake(&mut WindowsTvControls::new(tv))?;println!("Wake request sent: {} request(s) for {} target(s). The TV’s power state has not been verified.",report.packets,report.targets);}
    "off"=>{let report=tv_control::power(&mut WindowsTvControls::new(tv),PowerAction::Off)?;println!("{}\n{}",report.message,report.details);}
    "status"=>{
     let snapshot=tv_control::snapshot(&mut WindowsTvControls::new(tv));
     if snapshot.power==tv_control::PowerState::Unknown&&snapshot.inputs.is_empty()&&!snapshot.notes.is_empty(){return Err(snapshot.notes.join("\n"));}
     println!("Power: {}",snapshot.power.label());
     let current=snapshot.current_input.as_ref().and_then(|id|snapshot.inputs.iter().find(|i|&i.id==id)).map_or("Unknown",|i|i.label.as_str());
     println!("Current input: {current}");
     for input in snapshot.inputs{println!("{} — {}",input.id,input.label);}
     for note in snapshot.notes{println!("{note}");}
    }
    "check"=>{let inputs=windows_tv::check(tv)?;println!("The TV responded. Its control connection is available.");for input in inputs{println!("{} — {}",input.id,input.label);}}
    "pair"|"learn"=>{
     let (updated,warning)=if command=="pair"{println!("Accept the pairing prompt on your TV. This may take up to 45 seconds after connecting.");windows_tv::pair_with_details(tv)?}
     else{let details=windows_tv::inspect(tv)?;let mut updated=tv.clone();details.remember(&mut updated);(updated,details.network_warning)};
     let mut next=expected.clone();*next.iter_mut().find(|t|t.id==tv.id).ok_or("The saved TV is missing.")?=updated;tvs::save(&path,&expected,&next)?;
     println!("{}",if command=="pair"{"TV paired. Input and wake details saved where available."}else{"TV input and wake details saved."});if let Some(w)=warning{println!("{w}");}
    }
    _=>unreachable!(),
   }Ok(0)
  }
  _=>Err("Use: director tv list | pair <name> | check <name> | learn <name> | wake <name> | status <name> | off <name> | input <name> <input-id> | report <name>".into()),
 }
}
