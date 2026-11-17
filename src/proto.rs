use crate::engine::EngineStatus;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TuneResult {
    pub id: String,
    pub name: String,
    pub ok: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TuneState {
    pub running: bool,
    pub step: String,
    pub baseline: Option<usize>,
    pub results: Vec<TuneResult>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppStatus {
    pub enabled: bool,
    pub engine: EngineStatus,
    pub strategy_name: String,
    pub tune: TuneState,
    pub config_rev: u64,
    pub tray: bool,
    pub update: crate::update::UpdateState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cmd {
    SetEnabled(bool),
    Reload,
    AutoTune,
    Accept,
    CheckUpdate,
    InstallUpdate,
    Quit,
}
