use super::{args::Args, ipc::Client, parser::CommandParser};
use anyhow::bail;
use serde_json::Value;

pub(crate) struct InteractiveCommandClient {}
impl InteractiveCommandClient {
    pub fn call(args: Args) -> anyhow::Result<()> {
        let c = CommandParser::new(args.clone())?;
        let data = serde_json::to_string(&c).unwrap();
        // println!("request: {}", data);
        let response = Client::send(data.clone());
        match response {
            Ok(response) => {
                // println!("response: {}", response);
                match serde_json::from_str(&response) {
                    Ok(s) => {
                        match s {
                            Value::Array(array) => {
                                let status = &array[0];
                                let result = &array[1];
                                match status {
                                    Value::Number(n) => {
                                        let n = n.as_u64().unwrap();
                                        match n {
                                            0 => {
                                                match result {
                                                    Value::Null => {}
                                                    Value::Bool(_)
                                                    | Value::Number(_)
                                                    | Value::String(_)
                                                    | Value::Object(_)
                                                    | Value::Array(_) => {
                                                        println!("{}", result)
                                                    }
                                                }
                                                // println!("{result}");
                                                Ok(())
                                            }
                                            1 => bail!("{result}"),
                                            _ => bail!("qtile should return 0/1"),
                                        }
                                    }
                                    Value::Null
                                    | Value::Bool(_)
                                    | Value::String(_)
                                    | Value::Array(_)
                                    | Value::Object(_) => bail!("bad response by qtile!?"),
                                }
                            }
                            Value::Null
                            | Value::Bool(_)
                            | Value::String(_)
                            | Value::Object(_)
                            | Value::Number(_) => {
                                bail!("bad response by qtile!?")
                            }
                        }
                    }
                    Err(err) => bail!("{err}"),
                }
            }
            Err(_) => todo!(),
        }
    }
}
