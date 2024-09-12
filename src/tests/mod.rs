use anyhow::bail;

use crate::utils::{
    args::Args,
    client::{InteractiveCommandClient, ShellClient},
};

#[test]
fn interactive_command_client_x11() -> anyhow::Result<()> {
    let ret = InteractiveCommandClient::call(
        Some(vec!["root".to_string()]),
        Some("qtile_info".to_string()),
        Some(vec![]),
        false,
    );
    match ret {
        Ok(s) => println!("{:#}", s),
        Err(e) => bail!(e),
    }
    Ok(())
}

#[test]
fn shell_client_x11() -> anyhow::Result<()> {
    let args = Args {
        command: crate::utils::args::Commands::CmdObj {
            object: None,
            function: Some("qtile_info".to_owned()),
            args: None,
            info: false,
        },
    };
    ShellClient::call(args)
}
