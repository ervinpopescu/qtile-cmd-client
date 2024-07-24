#![allow(dead_code, unused_variables, unreachable_code, unused_imports)]
pub(crate) mod utils;
#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::utils::{
        args::Args,
        client::{InteractiveCommandClient, ShellClient},
    };

    #[test]
    fn main() -> anyhow::Result<()> {
        let args = Args::parse();
        // println!("{:#?}", args);
        let _res = ShellClient::call(args);
        InteractiveCommandClient::call(
            Some(vec!["root".to_string()]),
            Some("qtile_info".to_string()),
            Some(vec![]),
            false,
        )
    }
}
