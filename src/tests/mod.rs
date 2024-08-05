use crate::utils::{
    args::Args,
    client::{InteractiveCommandClient, ShellClient},
};

#[test]
fn interactive_command_client() {
    assert!(InteractiveCommandClient::call(
        Some(vec!["root".to_string()]),
        Some("qtile_info".to_string()),
        Some(vec![]),
        false,
    )
    .is_ok());
}

#[test]
fn shell_client() {
    let args = Args {
        command: crate::utils::args::Commands::CmdObj {
            object: None,
            function: Some("qtile_info".to_owned()),
            args: None,
            info: false,
        },
    };
    assert!(ShellClient::call(args).is_ok());
}
