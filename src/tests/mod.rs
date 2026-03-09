use anyhow::bail;

use crate::utils::client::{CallResult, CommandQuery, QtileClient};

pub mod framing;

#[test]
#[ignore = "requires live Qtile socket"]
fn qtile_info() -> anyhow::Result<()> {
    let client = QtileClient::new(false);
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
fn qtile_info_framed() -> anyhow::Result<()> {
    // This test specifically exercises the new framing protocol
    let client = QtileClient::new(true);
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
    let client = QtileClient::new(false);
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
#[ignore = "requires live Qtile socket"]
fn list_commands_framed() -> anyhow::Result<()> {
    let client = QtileClient::new(true);
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
    let client = QtileClient::new(false);
    let query = CommandQuery::new()
        .object(vec!["nonexistent_object".to_string()])
        .function("status".to_string());
    let ret = client.call(query);
    assert!(ret.is_err());
}

#[test]
fn test_invalid_function() {
    let client = QtileClient::new(false);
    let query = CommandQuery::new().function("nonexistent_function".to_string());
    let ret = client.call(query);
    assert!(ret.is_err());
}

#[test]
#[ignore = "requires live Qtile socket"]
fn test_repl_help_behavior() -> anyhow::Result<()> {
    let client = QtileClient::new(true);
    let query = CommandQuery::new().function("help".to_string());
    let ret = client.call(query);
    match ret {
        Ok(CallResult::Text(t)) => {
            assert!(!t.is_empty());
        }
        Ok(CallResult::Value(v)) => {
            bail!("Expected Text help, got Value: {v:?}");
        }
        Err(e) => bail!("Help failed: {e}"),
    }
    Ok(())
}

#[test]
#[ignore = "requires live Qtile socket"]
fn test_repl_help_behavior_unframed() -> anyhow::Result<()> {
    let client = QtileClient::new(false);
    let query = CommandQuery::new().function("help".to_string());
    let ret = client.call(query);
    match ret {
        Ok(CallResult::Text(t)) => {
            assert!(!t.is_empty());
        }
        Ok(CallResult::Value(v)) => {
            bail!("Expected Text help, got Value: {v:?}");
        }
        Err(e) => bail!("Help failed: {e}"),
    }
    Ok(())
}
