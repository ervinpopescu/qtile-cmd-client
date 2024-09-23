use anyhow::bail;

use crate::utils::client::InteractiveCommandClient;

#[test]
fn qtile_info() -> anyhow::Result<()> {
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
