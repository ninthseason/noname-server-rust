use serde_json::Value;
use uuid::Uuid;

pub fn generate_wsid() -> String {
    return Uuid::new_v4().to_string();
}

pub fn parse_command(cmd: &str) -> Vec<String> {
    let cmd: Value = serde_json::from_str(cmd).unwrap();
    let result = cmd.as_array().unwrap().iter().map(|x| match x {
        Value::String(s) => s.clone(),
        _ => x.to_string()
        
    }).collect();
    // println!("{:?}", result);
    return result;

}

pub fn espace_json(text: &str) -> String {
    text.replace("\\", "\\\\").replace("\"", "\\\"")
}

pub fn parse_key_from_token(tok: &str) -> String {
    let info: Value = serde_json::from_str(tok).unwrap();
    info.as_array().unwrap()[0].as_str().unwrap().to_string()
}

pub fn trim_nickname(name: &str) -> String {
    if name.len() > 12 {
        name[..12].to_string()
    } else {
        name.to_string()
    }
}