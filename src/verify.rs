// SPDX-License-Identifier: GPL-3.0-only
//! Real profile transactions against the isolated core launched by tools/check_runtime.py.
use crate::{api::Api, profile, settings::Preferences, window::LiveState};
use std::sync::Mutex;

pub fn run(url: &str) -> Result<(), String> {
    if std::env::var_os("PURR_CONFIG_DIR").is_none() {
        return Err("Verification requires an isolated PURR_CONFIG_DIR".into());
    }
    let dir = crate::config::app_config_dir()?;
    if !dir.join("purr-validation-fixture").is_file() {
        return Err("Isolated validation marker missing".into());
    }
    let settings = crate::config::prepare()?;
    let api = Api::new(settings.secret);
    let state = Mutex::new(LiveState {
        preferences: Preferences::load(&dir)?,
        ..Default::default()
    });
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e|e.to_string())?.block_on(async {
        let port=state.lock().unwrap().preferences.mixed_port;
        for (key,value) in [("ipv6","false"),("allow-lan","true"),("dns","8.8.8.8"),("block-ads","false"),("block-ads","true")] {
            profile::update(&api,&state,key,value).await?;
        }
        let value:serde_yaml::Value=serde_yaml::from_str(&std::fs::read_to_string(dir.join("config.yaml")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        require(value["ipv6"].as_bool()==Some(false),"IPv6 edit persisted")?;
        require(value["allow-lan"].as_bool()==Some(true),"LAN edit persisted")?;
        require(value["dns"]["nameserver"][0].as_str()==Some("https://dns.google/dns-query"),"DNS changed to requested DoH endpoint")?;
        require(api.rules().await?["rules"].as_array().is_some_and(|v|v.len()==2005),"four ad rules reached live core")?;
        let probe=std::net::TcpListener::bind("127.0.0.1:0").map_err(|e|e.to_string())?;
        let temporary=probe.local_addr().map_err(|e|e.to_string())?.port();
        drop(probe);
        profile::update(&api,&state,"mixed-port",&temporary.to_string()).await?;
        require(std::net::TcpStream::connect(("127.0.0.1",temporary)).is_ok(),"changed proxy listener opened")?;
        profile::update(&api,&state,"mixed-port",&port.to_string()).await?;
        for (key,value) in [("ipv6","true"),("allow-lan","false"),("dns","1.1.1.1")] {profile::update(&api,&state,key,value).await?;}
        let before=std::fs::read(dir.join("config.yaml")).map_err(|e|e.to_string())?;
        require(profile::update(&api,&state,"mixed-port","70000").await.is_err(),"invalid port rejected")?;
        let occupied=std::net::TcpListener::bind("127.0.0.1:0").map_err(|e|e.to_string())?;
        require(profile::update(&api,&state,"mixed-port",&occupied.local_addr().map_err(|e|e.to_string())?.port().to_string()).await.is_err(),"occupied port rejected")?;
        require(profile::import(&api,&state,&format!("{url}/invalid-profile")).await.is_err(),"invalid subscription rejected")?;
        require(before==std::fs::read(dir.join("config.yaml")).map_err(|e|e.to_string())?,"failed edits preserved exact previous config")?;
        profile::import(&api,&state,&format!("{url}/valid-profile")).await?;
        require(api.rules().await?["rules"].as_array().is_some_and(|v|v.len()==2005),"validated subscription loaded with ad preference")?;
        let saved=Preferences::load(&dir)?;
        require(saved.subscription.as_deref()==Some(&format!("{url}/valid-profile")),"subscription persisted")?;
        require(saved.mixed_port==port && !saved.system_proxy,"local listener and proxy preferences preserved")?;
        println!("backend: live settings, port rebinding, DNS, ad rules, subscription import and invalid-edit preservation passed");
        Ok(())
    })
}
fn require(ok: bool, label: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("Backend check failed: {label}"))
    }
}
