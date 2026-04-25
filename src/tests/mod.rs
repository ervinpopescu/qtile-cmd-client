use anyhow::bail;

use crate::utils::client::{CallResult, CommandQuery, QtileClient};

#[test]
#[ignore = "requires live Qtile socket"]
fn qtile_info() -> anyhow::Result<()> {
    let client = QtileClient::new();
    let query = CommandQuery::new()
        .object(vec!["root".to_string()])
        .function("qtile_info".to_string())
        .args(vec![]);
    let ret = client.call(query);
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
#[ignore = "requires live Qtile socket"]
fn list_commands() -> anyhow::Result<()> {
    let client = QtileClient::new();
    let query = CommandQuery::new().function("commands".to_string());
    let ret = client.call(query);
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
    let client = QtileClient::new();
    let query = CommandQuery::new()
        .object(vec!["nonexistent_object".to_string()])
        .function("status".to_string());
    let ret = client.call(query);
    assert!(ret.is_err());
}

#[test]
fn test_invalid_function() {
    let client = QtileClient::new();
    let query = CommandQuery::new().function("nonexistent_function".to_string());
    let ret = client.call(query);
    assert!(ret.is_err());
}
