use anyhow::bail;

use crate::utils::client::{CallResult, InteractiveCommandClient};

#[test]
fn qtile_info() -> anyhow::Result<()> {
    let ret = InteractiveCommandClient::call(
        Some(vec!["root".to_string()]),
        Some("qtile_info".to_string()),
        Some(vec![]),
        false,
    );
    match ret {
        Ok(CallResult::Value(s)) => {
            println!("{s:#}");
            assert!(s.is_object());
        }
        Ok(CallResult::Text(t)) => {
            bail!("Expected Value, got Text: {t}");
        }
        Err(e) => bail!(e),
    }
    Ok(())
}

#[test]
fn list_commands() -> anyhow::Result<()> {
    let ret = InteractiveCommandClient::call(None, Some("commands".to_string()), None, false);
    match ret {
        Ok(CallResult::Value(s)) => {
            assert!(s.is_array());
        }
        Ok(CallResult::Text(t)) => {
            assert!(!t.is_empty());
        }
        Err(e) => bail!(e),
    }
    Ok(())
}

#[test]
fn test_invalid_object() {
    let ret = InteractiveCommandClient::call(
        Some(vec!["nonexistent_object".to_string()]),
        Some("status".to_string()),
        None,
        false,
    );
    assert!(ret.is_err());
}

#[test]
fn test_invalid_function() {
    let ret =
        InteractiveCommandClient::call(None, Some("nonexistent_function".to_string()), None, false);
    assert!(ret.is_err());
}
